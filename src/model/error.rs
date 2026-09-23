//! The error taxonomy (docs/architecture.md §6.3).
//!
//! One error type, [`AppError`], crosses layers, and it distinguishes what the
//! user can act on. [`ParseError`] is the rule-language error, carrying a byte
//! offset so the interface can put a caret under the offending token.

use std::fmt;

use super::access::AccessState;

/// Why rule text could not be parsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ParseErrorKind {
    /// The input is longer than the parser accepts.
    TooLong,
    /// Parentheses are nested deeper than the parser accepts.
    TooDeep,
    /// The input is empty, or has no content before a comment.
    Empty,
    /// A `"` string was not closed.
    UnterminatedString,
    /// A `\` escape inside a string is not one the rule language knows.
    InvalidEscape,
    /// A `(` was not closed.
    UnterminatedParameter,
    /// The input ended where more was required.
    UnexpectedEnd,
    /// A token appeared where it is not allowed.
    UnexpectedToken,
    /// The first word is not a valid target for this entry point.
    InvalidTarget,
    /// The word is not one of the nine attribute names.
    UnknownAttribute,
    /// The same attribute appears twice.
    DuplicateAttribute,
    /// A value has the wrong shape for its attribute.
    InvalidValue,
    /// The word is not a known condition.
    UnknownCondition,
    /// A condition's parameter is missing, unexpected, or malformed.
    InvalidConditionParameter,
    /// A `{ … }` set holds more values than the parser accepts.
    TooManyValues,
}

impl ParseErrorKind {
    /// A short English description.
    #[must_use]
    pub const fn describe(self) -> &'static str {
        match self {
            Self::TooLong => "the rule is too long",
            Self::TooDeep => "parentheses are nested too deeply",
            Self::Empty => "the rule is empty",
            Self::UnterminatedString => "a quoted string is not closed",
            Self::InvalidEscape => "invalid escape sequence in a quoted string",
            Self::UnterminatedParameter => "a parenthesis is not closed",
            Self::UnexpectedEnd => "the rule ends too early",
            Self::UnexpectedToken => "unexpected text",
            Self::InvalidTarget => "expected allow, block, or reject",
            Self::UnknownAttribute => "unknown attribute",
            Self::DuplicateAttribute => "the same attribute appears twice",
            Self::InvalidValue => "invalid value for this attribute",
            Self::UnknownCondition => "unknown condition",
            Self::InvalidConditionParameter => "invalid condition parameter",
            Self::TooManyValues => "too many values in one set",
        }
    }
}

/// A rule-language error with its position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ParseError {
    /// What went wrong.
    pub kind: ParseErrorKind,
    /// Byte offset into the input, for caret placement.
    pub offset: usize,
}

impl ParseError {
    /// Builds an error.
    #[must_use]
    pub const fn new(kind: ParseErrorKind, offset: usize) -> Self {
        Self { kind, offset }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at byte {}", self.kind.describe(), self.offset)
    }
}

impl std::error::Error for ParseError {}

/// The one error type that crosses layers.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AppError {
    /// One of the three checkpoints refused. Reopens the diagnostic panel.
    #[error("access denied: {0}")]
    Denied(AccessState),
    /// The bus or the bridge cannot be reached.
    #[error("the daemon is unreachable: {0}")]
    Unreachable(String),
    /// The daemon understood the request and refused it.
    #[error("the daemon rejected the request: {0}")]
    Rejected(String),
    /// Local validation failed before anything was sent.
    #[error("could not parse the rule language: {0}")]
    Parse(#[from] ParseError),
    /// The user cancelled the operation. The daemon may still have acted.
    #[error("the operation was cancelled")]
    Cancelled,
    /// The ruleset changed underneath the operation: re-read, don't retry.
    #[error("the ruleset changed underneath this operation")]
    Stale,
}
