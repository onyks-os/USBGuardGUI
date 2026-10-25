use super::{AccessState, Capabilities};

#[test]
fn writes_start_optimistic_reads_pessimistic() {
    let caps = Capabilities::default();
    assert!(caps.modify_devices && caps.modify_rules && caps.set_parameters);
    assert!(!caps.list_devices && !caps.list_rules && !caps.get_parameters);
}

#[test]
fn denied_classification() {
    assert!(AccessState::DeniedByIpcAcl.is_denied());
    assert!(!AccessState::BridgeNotInstalled.is_denied());
    assert!(
        !AccessState::Ok {
            caps: Capabilities::default()
        }
        .is_denied()
    );
}

#[test]
fn denial_details_and_polkit_actions_remain_visible() {
    use super::AccessState;
    assert_eq!(
        AccessState::DeniedByPolkit {
            action: Some("org.usbguard.test".into())
        }
        .to_string(),
        "denied by Polkit (org.usbguard.test)"
    );
    assert_eq!(
        AccessState::DeniedUnattributed {
            detail: "new refusal".into()
        }
        .to_string(),
        "access denied: new refusal"
    );
    assert_eq!(
        AccessState::DeniedUnattributed {
            detail: String::new()
        }
        .to_string(),
        "access denied"
    );
}

#[test]
fn write_capabilities_are_independent_and_denial_names_are_explicit() {
    let mut caps = Capabilities {
        modify_devices: false,
        modify_rules: false,
        set_parameters: false,
        ..Capabilities::default()
    };
    assert!(!caps.can_write());
    caps.set_parameters = true;
    assert!(caps.can_write());
    for (state, text) in [
        (AccessState::DeniedByBusPolicy, "D-Bus bus policy"),
        (AccessState::DeniedByIpcAcl, "IPC access control"),
        (AccessState::NoPolkitAgent, "no Polkit authentication agent"),
    ] {
        assert!(state.to_string().contains(text));
    }
}
