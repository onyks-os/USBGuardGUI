//! The typed client: the proxies of [`super::proxies`] wrapped in the domain
//! types of [`crate::model`] (docs/architecture.md §8).
//!
//! This is the only module that turns a raw `u32` into a [`DeviceId`] or a
//! [`RuleId`], a `u32` target into a [`Target`], or a `bool` into a
//! [`Persistence`]. Everything above it never sees a wire value.

use tracing::{debug, instrument};
use zbus::Connection;

use super::errors::app_error;
use super::proxies::{UsbGuardDevicesProxy, UsbGuardPolicyProxy, UsbGuardProxy};
use crate::model::{
    AppError, Device, DeviceId, DevicePolicy, Parameter, Persistence, RemoveOutcome, Rule,
    RuleHandle, RuleId,
};
use crate::rules::{parse_device, render_checked};

/// Query matching every device.
pub const QUERY_ALL_DEVICES: &str = "match";

/// Label filter matching every rule. `listRules` takes a *label*, and the
/// empty string disables the filter (Phase 0 finding; see proxies.rs).
pub const LABEL_ALL_RULES: &str = "";

/// A connected USBGuard client.
///
/// Cheap to clone: the connection and the proxies are reference-counted.
#[derive(Debug, Clone)]
pub struct Client {
    connection: Connection,
    root: UsbGuardProxy<'static>,
    devices: UsbGuardDevicesProxy<'static>,
    policy: UsbGuardPolicyProxy<'static>,
}

impl Client {
    /// Connects to the system bus.
    ///
    /// # Errors
    ///
    /// [`AppError::Unreachable`] when there is no system bus.
    pub async fn connect_system() -> Result<Self, AppError> {
        let connection = super::bus::connect().await.map_err(|e| app_error(&e))?;
        Self::new(&connection).await
    }

    /// Wraps an existing connection — the system bus in production, a
    /// private connection to the mock daemon in tests.
    ///
    /// # Errors
    ///
    /// Returns an error only if the proxies cannot be built, which does not
    /// involve any bus round trip.
    pub async fn new(connection: &Connection) -> Result<Self, AppError> {
        let build = |e: zbus::Error| app_error(&e);
        Ok(Self {
            connection: connection.clone(),
            root: UsbGuardProxy::builder(connection)
                .cache_properties(zbus::proxy::CacheProperties::No)
                .build()
                .await
                .map_err(build)?,
            devices: UsbGuardDevicesProxy::builder(connection)
                .cache_properties(zbus::proxy::CacheProperties::No)
                .build()
                .await
                .map_err(build)?,
            policy: UsbGuardPolicyProxy::builder(connection)
                .cache_properties(zbus::proxy::CacheProperties::No)
                .build()
                .await
                .map_err(build)?,
        })
    }

    /// The underlying connection.
    pub const fn connection(&self) -> &Connection {
        &self.connection
    }

    /// The root-interface proxy, for signal subscriptions.
    #[must_use]
    pub const fn root_proxy(&self) -> &UsbGuardProxy<'static> {
        &self.root
    }

    /// The devices proxy, for signal subscriptions.
    #[must_use]
    pub const fn devices_proxy(&self) -> &UsbGuardDevicesProxy<'static> {
        &self.devices
    }

    /// Every device the daemon knows, in no particular order (§2.4.9).
    ///
    /// # Errors
    ///
    /// Any D-Bus failure, classified per §6.3.
    #[instrument(skip(self), level = "debug")]
    pub async fn list_devices(&self) -> Result<Vec<Device>, AppError> {
        let raw = self
            .devices
            .list_devices(QUERY_ALL_DEVICES)
            .await
            .map_err(|e| app_error(&e))?;
        debug!(count = raw.len(), "listDevices");
        Ok(raw
            .into_iter()
            .map(|(id, text)| parse_device(DeviceId::new(id), text))
            .collect())
    }

    /// The whole ruleset, in evaluation order.
    ///
    /// # Errors
    ///
    /// Any D-Bus failure, classified per §6.3.
    #[instrument(skip(self), level = "debug")]
    pub async fn list_rules(&self) -> Result<Vec<RuleHandle>, AppError> {
        let raw = self
            .policy
            .list_rules(LABEL_ALL_RULES)
            .await
            .map_err(|e| app_error(&e))?;
        debug!(count = raw.len(), "listRules");
        Ok(to_handles(raw))
    }

    /// Reads a runtime parameter.
    ///
    /// # Errors
    ///
    /// Any D-Bus failure, classified per §6.3.
    #[instrument(skip(self), level = "debug")]
    pub async fn get_parameter(&self, parameter: Parameter) -> Result<String, AppError> {
        self.root
            .get_parameter(parameter.name())
            .await
            .map_err(|e| app_error(&e))
    }

    /// Sets a runtime parameter and returns its previous value.
    ///
    /// # Errors
    ///
    /// [`AppError::Rejected`] for a value outside the parameter's closed set,
    /// before anything is sent; otherwise any D-Bus failure.
    #[instrument(skip(self), level = "debug")]
    pub async fn set_parameter(
        &self,
        parameter: Parameter,
        value: &str,
    ) -> Result<String, AppError> {
        if !parameter.allowed_values().contains(&value) {
            return Err(AppError::Rejected(format!(
                "{value:?} is not a valid value for {parameter}"
            )));
        }
        self.root
            .set_parameter(parameter.name(), value)
            .await
            .map_err(|e| app_error(&e))
    }

    /// Allows, blocks, or rejects a device.
    ///
    /// Returns the id of the rule the daemon created or changed, which exists
    /// only for a permanent change (§2.4.8).
    ///
    /// # Errors
    ///
    /// Any D-Bus failure, classified per §6.3.
    #[instrument(skip(self), level = "debug")]
    pub async fn apply_device_policy(
        &self,
        device: DeviceId,
        policy: DevicePolicy,
        persistence: Persistence,
    ) -> Result<Option<RuleId>, AppError> {
        let rule_id = self
            .devices
            .apply_device_policy(device.get(), policy.wire(), persistence.as_permanent())
            .await
            .map_err(|e| app_error(&e))?;
        Ok(persistence.as_permanent().then_some(RuleId::new(rule_id)))
    }

    /// Appends a rule after `parent` — use [`RuleId::APPEND_LAST`] for the end
    /// of the ruleset (§2.4.6).
    ///
    /// The rule is rendered and re-parsed locally first (§7.2), so a rule the
    /// parser rejects never reaches the daemon and never raises a Polkit prompt.
    ///
    /// # Errors
    ///
    /// [`AppError::Parse`] if the rule does not survive its own round trip,
    /// [`AppError::Rejected`] for a rule without a policy target; otherwise
    /// any D-Bus failure.
    #[instrument(skip(self, rule), level = "debug")]
    pub async fn append_rule(
        &self,
        rule: &Rule,
        parent: RuleId,
        persistence: Persistence,
    ) -> Result<RuleId, AppError> {
        let text = render_checked(rule)?;
        // A policy rule needs allow/block/reject; `match` and partial rules
        // are query syntax only.
        crate::rules::parse_rule(&text)?;
        let id = self
            .policy
            .append_rule(&text, parent.get(), persistence.as_temporary())
            .await
            .map_err(|e| app_error(&e))?;
        Ok(RuleId::new(id))
    }

    /// Appends a rule after the rule `after` — or at the end when `None` —
    /// resolving `after` by its text immediately before the call, because its
    /// id may have changed since it was read (§2.4.2, §6.4).
    ///
    /// # Errors
    ///
    /// [`AppError::Stale`] if the anchor rule is gone or is no longer uniquely
    /// identified by text and position; otherwise as [`Client::append_rule`].
    pub async fn append_rule_after(
        &self,
        rule: &Rule,
        after: Option<&RuleHandle>,
        persistence: Persistence,
    ) -> Result<RuleId, AppError> {
        let parent = match after {
            None => RuleId::APPEND_LAST,
            Some(anchor) => {
                let current = self.list_rules().await?;
                match resolve(&current, &anchor.text) {
                    Resolution::Unique(id) => id,
                    Resolution::Ambiguous(candidates) => candidates
                        .iter()
                        .find(|c| c.position == anchor.position)
                        .map(|c| c.id)
                        .ok_or(AppError::Stale)?,
                    Resolution::Gone => return Err(AppError::Stale),
                }
            }
        };
        self.append_rule(rule, parent, persistence).await
    }

    /// Removes a rule safely (§6.4): the rule's text is its identity, and its
    /// id is re-resolved immediately before use.
    ///
    /// # Errors
    ///
    /// Any D-Bus failure, classified per §6.3.
    #[instrument(skip(self), level = "debug")]
    pub async fn remove_rule(&self, handle: &RuleHandle) -> Result<RemoveOutcome, AppError> {
        let current = self.list_rules().await?;
        match resolve(&current, &handle.text) {
            Resolution::Gone => Ok(RemoveOutcome::AlreadyGone),
            Resolution::Unique(id) => {
                self.policy
                    .remove_rule(id.get())
                    .await
                    .map_err(|e| app_error(&e))?;
                Ok(RemoveOutcome::Removed)
            }
            Resolution::Ambiguous(candidates) => Ok(RemoveOutcome::Ambiguous { candidates }),
        }
    }

    /// Removes one specific candidate chosen by the user from an
    /// [`RemoveOutcome::Ambiguous`] answer. The candidate is re-resolved by
    /// text *and* position; if either changed, nothing is removed.
    ///
    /// # Errors
    ///
    /// [`AppError::Stale`] if the ruleset changed since the candidates were
    /// listed; otherwise any D-Bus failure.
    pub async fn remove_rule_at(&self, candidate: &RuleHandle) -> Result<RemoveOutcome, AppError> {
        let current = self.list_rules().await?;
        let still_there = current.iter().find(|h| {
            h.position == candidate.position && h.text == candidate.text && h.id == candidate.id
        });
        let Some(target) = still_there else {
            return Err(AppError::Stale);
        };
        self.policy
            .remove_rule(target.id.get())
            .await
            .map_err(|e| app_error(&e))?;
        Ok(RemoveOutcome::Removed)
    }
}

/// Converts `listRules` output into handles, recording evaluation positions.
#[must_use]
pub fn to_handles(raw: Vec<(u32, String)>) -> Vec<RuleHandle> {
    raw.into_iter()
        .enumerate()
        .map(|(position, (id, text))| RuleHandle {
            id: RuleId::new(id),
            text,
            position,
        })
        .collect()
}

/// What a rule text resolves to in the current ruleset.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Resolution {
    Gone,
    Unique(RuleId),
    Ambiguous(Vec<RuleHandle>),
}

fn resolve(current: &[RuleHandle], text: &str) -> Resolution {
    let matches: Vec<RuleHandle> = current.iter().filter(|h| h.text == text).cloned().collect();
    match matches.as_slice() {
        [] => Resolution::Gone,
        [only] => Resolution::Unique(only.id),
        _ => Resolution::Ambiguous(matches),
    }
}

#[cfg(test)]
mod tests {
    use super::{Resolution, resolve, to_handles};
    use crate::model::RuleId;

    #[test]
    fn resolution_by_text() {
        let rules = to_handles(vec![
            (5, "allow id 1234:5678".into()),
            (9, "block".into()),
            (12, "allow id 1234:5678".into()),
        ]);
        assert_eq!(rules[2].position, 2);
        assert_eq!(resolve(&rules, "block"), Resolution::Unique(RuleId::new(9)));
        assert_eq!(resolve(&rules, "reject"), Resolution::Gone);
        let Resolution::Ambiguous(c) = resolve(&rules, "allow id 1234:5678") else {
            panic!("expected ambiguity");
        };
        assert_eq!(c.iter().map(|h| h.position).collect::<Vec<_>>(), vec![0, 2]);
    }
}
