//! The rule language as data (docs/architecture.md §2.6), plus rule identity
//! (§6.4).
//!
//! These types are pure data: parsing lives in [`crate::rules`], and rendering
//! back to text is their `Display` implementation, also in [`crate::rules`].

use std::borrow::Cow;

use super::ids::{InterfaceType, RuleId, UsbId};

/// The content of a quoted string in rule text, already unescaped.
///
/// Stored as bytes, not `String`: device names and serial numbers come from
/// descriptors the device itself supplies and are not guaranteed to be UTF-8
/// (§7.2). Conversion to text happens only at the presentation boundary, with
/// [`RuleString::to_string_lossy`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct RuleString(Vec<u8>);

impl RuleString {
    /// Wraps raw bytes.
    #[must_use]
    pub const fn from_bytes(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    /// The raw bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Text for display: invalid UTF-8 sequences become U+FFFD.
    #[must_use]
    pub fn to_string_lossy(&self) -> Cow<'_, str> {
        String::from_utf8_lossy(&self.0)
    }
}

impl From<&str> for RuleString {
    fn from(s: &str) -> Self {
        Self(s.as_bytes().to_vec())
    }
}

/// The first word of a rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RuleTarget {
    /// `allow`
    Allow,
    /// `block`
    Block,
    /// `reject`
    Reject,
    /// `match` — valid only in *queries*, never in policy text (§2.6).
    Match,
}

impl RuleTarget {
    /// The keyword as written in rule text.
    #[must_use]
    pub const fn keyword(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Block => "block",
            Self::Reject => "reject",
            Self::Match => "match",
        }
    }

    /// The inverse of [`RuleTarget::keyword`].
    #[must_use]
    pub fn from_keyword(word: &[u8]) -> Option<Self> {
        Some(match word {
            b"allow" => Self::Allow,
            b"block" => Self::Block,
            b"reject" => Self::Reject,
            b"match" => Self::Match,
            _ => return None,
        })
    }
}

/// The nine attribute names of the rule language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AttributeName {
    /// `id` — `vendor:product`.
    Id,
    /// `hash` — hash of the device descriptor.
    Hash,
    /// `parent-hash` — hash of the parent device.
    ParentHash,
    /// `name` — the product string.
    Name,
    /// `serial` — the serial number string.
    Serial,
    /// `via-port` — the port the device is attached to.
    ViaPort,
    /// `with-interface` — interface types, `cc:ss:pp`.
    WithInterface,
    /// `with-connect-type` — e.g. `hotplug`.
    WithConnectType,
    /// `label` — metadata only; never takes part in matching.
    Label,
}

impl AttributeName {
    /// Every attribute name, in the order the daemon renders them.
    pub const ALL: [Self; 9] = [
        Self::Id,
        Self::Serial,
        Self::Name,
        Self::Hash,
        Self::ParentHash,
        Self::ViaPort,
        Self::WithInterface,
        Self::WithConnectType,
        Self::Label,
    ];

    /// The keyword as written in rule text.
    #[must_use]
    pub const fn keyword(self) -> &'static str {
        match self {
            Self::Id => "id",
            Self::Hash => "hash",
            Self::ParentHash => "parent-hash",
            Self::Name => "name",
            Self::Serial => "serial",
            Self::ViaPort => "via-port",
            Self::WithInterface => "with-interface",
            Self::WithConnectType => "with-connect-type",
            Self::Label => "label",
        }
    }

    /// The inverse of [`AttributeName::keyword`].
    #[must_use]
    pub fn from_keyword(word: &[u8]) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|a| a.keyword().as_bytes() == word)
    }
}

/// The operators that may precede a `{ … }` set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SetOperator {
    /// The device set contains all listed values.
    AllOf,
    /// The device set contains at least one listed value.
    OneOf,
    /// The device set contains no listed value.
    NoneOf,
    /// The device set is exactly the listed set. Implied when absent.
    Equals,
    /// Exactly the listed set, in the listed order.
    EqualsOrdered,
    /// The device set is a subset of the listed values.
    MatchAll,
}

impl SetOperator {
    /// Every operator.
    pub const ALL: [Self; 6] = [
        Self::AllOf,
        Self::OneOf,
        Self::NoneOf,
        Self::Equals,
        Self::EqualsOrdered,
        Self::MatchAll,
    ];

    /// The keyword as written in rule text.
    #[must_use]
    pub const fn keyword(self) -> &'static str {
        match self {
            Self::AllOf => "all-of",
            Self::OneOf => "one-of",
            Self::NoneOf => "none-of",
            Self::Equals => "equals",
            Self::EqualsOrdered => "equals-ordered",
            Self::MatchAll => "match-all",
        }
    }

    /// The inverse of [`SetOperator::keyword`].
    #[must_use]
    pub fn from_keyword(word: &[u8]) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|o| o.keyword().as_bytes() == word)
    }
}

/// One attribute value. Which variant is valid depends on the attribute name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AttributeValue {
    /// For `id`.
    UsbId(UsbId),
    /// For `with-interface`.
    Interface(InterfaceType),
    /// For every other attribute: a quoted string.
    String(RuleString),
}

/// Either one bare value, or a `{ … }` set with an optional operator.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AttributeValues {
    /// `name "Keyboard"`
    Single(AttributeValue),
    /// `with-interface one-of { 03:00:01 03:01:01 }`. `operator` is `None`
    /// when absent in the text, which means `equals`; the absence is kept so
    /// that rendering reproduces the input.
    Set {
        /// The operator, if written.
        operator: Option<SetOperator>,
        /// The listed values, in order.
        values: Vec<AttributeValue>,
    },
}

impl AttributeValues {
    /// Every value, regardless of form.
    #[must_use]
    pub fn as_slice(&self) -> &[AttributeValue] {
        match self {
            Self::Single(v) => std::slice::from_ref(v),
            Self::Set { values, .. } => values,
        }
    }
}

/// One `name value` or `name [op] { values }` pair.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Attribute {
    /// Which attribute.
    pub name: AttributeName,
    /// Its value or values.
    pub values: AttributeValues,
}

/// The condition names of the `if` clause.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConditionName {
    /// `localtime(HH:MM[:SS][-HH:MM[:SS]])`
    LocalTime,
    /// `allowed-matches(query)`
    AllowedMatches,
    /// `rule-applied` or `rule-applied(duration)`
    RuleApplied,
    /// `rule-evaluated` or `rule-evaluated(duration)`
    RuleEvaluated,
    /// `random` or `random(p)`
    Random,
    /// `true`
    True,
    /// `false`
    False,
}

impl ConditionName {
    /// Every condition name.
    pub const ALL: [Self; 7] = [
        Self::LocalTime,
        Self::AllowedMatches,
        Self::RuleApplied,
        Self::RuleEvaluated,
        Self::Random,
        Self::True,
        Self::False,
    ];

    /// The keyword as written in rule text.
    #[must_use]
    pub const fn keyword(self) -> &'static str {
        match self {
            Self::LocalTime => "localtime",
            Self::AllowedMatches => "allowed-matches",
            Self::RuleApplied => "rule-applied",
            Self::RuleEvaluated => "rule-evaluated",
            Self::Random => "random",
            Self::True => "true",
            Self::False => "false",
        }
    }

    /// The inverse of [`ConditionName::keyword`].
    #[must_use]
    pub fn from_keyword(word: &[u8]) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|c| c.keyword().as_bytes() == word)
    }
}

/// One condition, e.g. `!localtime(08:00-18:00)`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Condition {
    /// Preceded by `!`.
    pub negated: bool,
    /// Which condition.
    pub name: ConditionName,
    /// The text between the parentheses, verbatim, when present. The parser
    /// validates its shape for each condition; it is kept as written so that
    /// rendering reproduces it exactly.
    pub parameter: Option<Vec<u8>>,
}

/// The `if …` clause.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ConditionClause {
    /// `if [!]condition`
    Single(Condition),
    /// `if [op] { [!]condition … }`
    Set {
        /// The operator, if written.
        operator: Option<SetOperator>,
        /// The listed conditions, in order.
        conditions: Vec<Condition>,
    },
}

/// A parsed rule, partial rule, or query.
///
/// The three differ only in which targets are allowed, which is the job of
/// the three parser entry points in [`crate::rules`]:
///
/// | Entry point     | `target`                                 |
/// |-----------------|------------------------------------------|
/// | `parse_rule`    | `Some(Allow \| Block \| Reject)`         |
/// | `parse_partial` | `None`                                   |
/// | `parse_query`   | any, including `Some(Match)`             |
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Rule {
    /// The target, absent for a partial rule.
    pub target: Option<RuleTarget>,
    /// Attributes, in the order written.
    pub attributes: Vec<Attribute>,
    /// The `if` clause.
    pub condition: Option<ConditionClause>,
    /// The text after `#`, without the `#`.
    pub comment: Option<Vec<u8>>,
}

impl Rule {
    /// The attribute with the given name, if the rule has it.
    #[must_use]
    pub fn attribute(&self, name: AttributeName) -> Option<&Attribute> {
        self.attributes.iter().find(|a| a.name == name)
    }

    /// The first string value of the given attribute, if any.
    #[must_use]
    pub fn string_value(&self, name: AttributeName) -> Option<&RuleString> {
        self.attribute(name)?
            .values
            .as_slice()
            .iter()
            .find_map(|v| match v {
                AttributeValue::String(s) => Some(s),
                _ => None,
            })
    }
}

/// A rule as read from the daemon, with the identity protocol of §6.4.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleHandle {
    /// Volatile: valid only while the ruleset is unchanged.
    pub id: RuleId,
    /// Stable identity: the canonical text as returned by `listRules`.
    pub text: String,
    /// Evaluation-order position at the time of reading, for disambiguation.
    pub position: usize,
}

/// The outcome of a safe rule removal (§6.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoveOutcome {
    /// The rule was removed.
    Removed,
    /// No rule with this text exists any more: someone else removed it.
    AlreadyGone,
    /// Several rules share this text: the user must choose.
    Ambiguous {
        /// Every rule with the same text, with its current position.
        candidates: Vec<RuleHandle>,
    },
}
