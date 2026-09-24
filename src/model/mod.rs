//! Domain types shared across the application (docs/architecture.md §6).
//!
//! `model` depends on nothing else in the program — not on the parser, not on
//! zbus, not on GTK — so everything here is testable without a bus or a display.

pub mod access;
pub mod device;
pub mod error;
pub mod event;
pub mod ids;
pub mod parameter;
pub mod rule;
pub mod target;

pub use access::{AccessState, Capabilities};
pub use device::{Device, DeviceAttributes, DeviceDelta, PendingKind};
pub use error::{AppError, ParseError, ParseErrorKind};
pub use event::{OperationId, OperationOutcome, UiEvent};
pub use ids::{DeviceId, IdPart, InterfaceType, RuleId, UsbId};
pub use parameter::Parameter;
pub use rule::{
    Attribute, AttributeName, AttributeValue, AttributeValues, Condition, ConditionClause,
    ConditionName, RemoveOutcome, Rule, RuleHandle, RuleString, RuleTarget, SetOperator,
};
pub use target::{DeviceEvent, DevicePolicy, Persistence, Target};
