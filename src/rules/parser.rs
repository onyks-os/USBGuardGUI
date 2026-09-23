//! Recursive-descent parser over the token stream (docs/architecture.md §2.6, §7).
//!
//! Every byte this module sees originates outside the program — from the
//! daemon, and ultimately from a USB descriptor an attacker may control. So:
//! no `unwrap`, no `expect`, no slice indexing (enforced by the lints on
//! [`crate::rules`]), an explicit input-length bound checked first, and a
//! nesting bound checked by the lexer before any recursion.

use super::lexer::{Token, TokenKind, tokenize};
use crate::model::{
    Attribute, AttributeName, AttributeValue, AttributeValues, Condition, ConditionClause,
    ConditionName, InterfaceType, ParseError, ParseErrorKind, Rule, RuleString, RuleTarget,
    SetOperator, UsbId,
};

/// Longest input accepted, in bytes. Real device rules are a few hundred bytes;
/// the bound exists so that a hostile input costs bounded memory and time.
pub const MAX_INPUT_LEN: usize = 64 * 1024;

/// Most values accepted in one `{ … }` set.
pub const MAX_SET_VALUES: usize = 4096;

/// Which targets an entry point accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    /// Policy text: `allow | block | reject`, required.
    Rule,
    /// No target at all.
    Partial,
    /// Query text: any target including `match`, or none.
    Query,
}

/// Parses rule text under the given mode.
pub(crate) fn parse(input: &[u8], mode: Mode) -> Result<Rule, ParseError> {
    if input.len() > MAX_INPUT_LEN {
        return Err(ParseError::new(ParseErrorKind::TooLong, MAX_INPUT_LEN));
    }
    let tokens = tokenize(input)?;
    Parser {
        tokens: &tokens,
        pos: 0,
        end: input.len(),
    }
    .rule(mode)
}

struct Parser<'t, 'a> {
    tokens: &'t [Token<'a>],
    pos: usize,
    /// Offset reported for errors at end of input.
    end: usize,
}

impl<'a> Parser<'_, 'a> {
    fn peek(&self) -> Option<&Token<'a>> {
        self.tokens.get(self.pos)
    }

    fn next(&mut self) -> Result<&Token<'a>, ParseError> {
        let token = self
            .tokens
            .get(self.pos)
            .ok_or(ParseError::new(ParseErrorKind::UnexpectedEnd, self.end))?;
        self.pos += 1;
        Ok(token)
    }

    fn offset(&self) -> usize {
        self.peek().map_or(self.end, |t| t.offset)
    }

    fn rule(mut self, mode: Mode) -> Result<Rule, ParseError> {
        let mut rule = Rule {
            target: self.target(mode)?,
            ..Rule::default()
        };

        while let Some(token) = self.peek() {
            let offset = token.offset;
            match &token.kind {
                TokenKind::Word(b"if") => {
                    self.pos += 1;
                    rule.condition = Some(self.condition_clause()?);
                    break;
                }
                TokenKind::Word(word) => {
                    let name = AttributeName::from_keyword(word)
                        .ok_or(ParseError::new(ParseErrorKind::UnknownAttribute, offset))?;
                    if rule.attribute(name).is_some() {
                        return Err(ParseError::new(ParseErrorKind::DuplicateAttribute, offset));
                    }
                    self.pos += 1;
                    let values = self.attribute_values(name)?;
                    rule.attributes.push(Attribute { name, values });
                }
                TokenKind::Comment(_) => break,
                _ => return Err(ParseError::new(ParseErrorKind::UnexpectedToken, offset)),
            }
        }

        if let Some(token) = self.peek() {
            let TokenKind::Comment(text) = token.kind else {
                return Err(ParseError::new(
                    ParseErrorKind::UnexpectedToken,
                    token.offset,
                ));
            };
            rule.comment = Some(text.to_vec());
            self.pos += 1;
        }

        if rule.target.is_none() && rule.attributes.is_empty() && rule.condition.is_none() {
            return Err(ParseError::new(ParseErrorKind::Empty, 0));
        }
        Ok(rule)
    }

    fn target(&mut self, mode: Mode) -> Result<Option<RuleTarget>, ParseError> {
        let offset = self.offset();
        let target = match self.peek().map(|t| &t.kind) {
            Some(TokenKind::Word(word)) => RuleTarget::from_keyword(word),
            _ => None,
        };
        let allowed = match (mode, target) {
            (Mode::Rule, Some(RuleTarget::Allow | RuleTarget::Block | RuleTarget::Reject))
            | (Mode::Partial, None)
            | (Mode::Query, _) => true,
            (Mode::Rule, _) | (Mode::Partial, Some(_)) => false,
        };
        if !allowed {
            let kind = if self.peek().is_none() {
                ParseErrorKind::Empty
            } else {
                ParseErrorKind::InvalidTarget
            };
            return Err(ParseError::new(kind, offset));
        }
        if target.is_some() {
            self.pos += 1;
        }
        Ok(target)
    }

    fn attribute_values(&mut self, name: AttributeName) -> Result<AttributeValues, ParseError> {
        match self.peek().map(|t| &t.kind) {
            Some(TokenKind::LBrace) => self.set(name, None),
            Some(TokenKind::Word(word)) => {
                if let Some(op) = SetOperator::from_keyword(word) {
                    self.pos += 1;
                    self.set(name, Some(op))
                } else {
                    Ok(AttributeValues::Single(self.value(name)?))
                }
            }
            _ => Ok(AttributeValues::Single(self.value(name)?)),
        }
    }

    fn set(
        &mut self,
        name: AttributeName,
        operator: Option<SetOperator>,
    ) -> Result<AttributeValues, ParseError> {
        self.expect_lbrace()?;
        let mut values = Vec::new();
        loop {
            if matches!(self.peek().map(|t| &t.kind), Some(TokenKind::RBrace)) {
                self.pos += 1;
                return Ok(AttributeValues::Set { operator, values });
            }
            if values.len() >= MAX_SET_VALUES {
                return Err(ParseError::new(
                    ParseErrorKind::TooManyValues,
                    self.offset(),
                ));
            }
            values.push(self.value(name)?);
        }
    }

    fn expect_lbrace(&mut self) -> Result<(), ParseError> {
        let token = self.next()?;
        if token.kind == TokenKind::LBrace {
            Ok(())
        } else {
            Err(ParseError::new(
                ParseErrorKind::UnexpectedToken,
                token.offset,
            ))
        }
    }

    fn value(&mut self, name: AttributeName) -> Result<AttributeValue, ParseError> {
        let token = self.next()?;
        let invalid = ParseError::new(ParseErrorKind::InvalidValue, token.offset);
        match (name, &token.kind) {
            (AttributeName::Id, TokenKind::Word(w)) => {
                UsbId::parse(w).map(AttributeValue::UsbId).ok_or(invalid)
            }
            (AttributeName::WithInterface, TokenKind::Word(w)) => InterfaceType::parse(w)
                .map(AttributeValue::Interface)
                .ok_or(invalid),
            (
                AttributeName::Hash
                | AttributeName::ParentHash
                | AttributeName::Name
                | AttributeName::Serial
                | AttributeName::ViaPort
                | AttributeName::WithConnectType
                | AttributeName::Label,
                TokenKind::Str(s),
            ) => Ok(AttributeValue::String(RuleString::from_bytes(s.clone()))),
            _ => Err(invalid),
        }
    }

    fn condition_clause(&mut self) -> Result<ConditionClause, ParseError> {
        let operator = match self.peek().map(|t| &t.kind) {
            Some(TokenKind::Word(word)) => SetOperator::from_keyword(word),
            _ => None,
        };
        if operator.is_some() {
            self.pos += 1;
        }
        let braced = matches!(self.peek().map(|t| &t.kind), Some(TokenKind::LBrace));
        if !braced {
            if operator.is_some() {
                self.expect_lbrace()?;
            }
            return Ok(ConditionClause::Single(self.condition()?));
        }
        self.pos += 1;
        let mut conditions = Vec::new();
        loop {
            if matches!(self.peek().map(|t| &t.kind), Some(TokenKind::RBrace)) {
                self.pos += 1;
                return Ok(ConditionClause::Set {
                    operator,
                    conditions,
                });
            }
            if conditions.len() >= MAX_SET_VALUES {
                return Err(ParseError::new(
                    ParseErrorKind::TooManyValues,
                    self.offset(),
                ));
            }
            conditions.push(self.condition()?);
        }
    }

    fn condition(&mut self) -> Result<Condition, ParseError> {
        let negated = matches!(self.peek().map(|t| &t.kind), Some(TokenKind::Bang));
        if negated {
            self.pos += 1;
        }
        let token = self.next()?;
        let offset = token.offset;
        let TokenKind::Word(word) = token.kind else {
            return Err(ParseError::new(ParseErrorKind::UnexpectedToken, offset));
        };
        let name = ConditionName::from_keyword(word)
            .ok_or(ParseError::new(ParseErrorKind::UnknownCondition, offset))?;

        let parameter = match self.peek() {
            Some(Token {
                kind: TokenKind::Param(p),
                offset,
            }) => {
                let (p, offset) = (*p, *offset);
                self.pos += 1;
                Some((p, offset))
            }
            _ => None,
        };
        validate_condition(name, parameter)?;
        Ok(Condition {
            negated,
            name,
            parameter: parameter.map(|(p, _)| p.to_vec()),
        })
    }
}

/// Checks that a condition's parameter is present when required, absent when
/// forbidden, and well-formed.
fn validate_condition(
    name: ConditionName,
    parameter: Option<(&[u8], usize)>,
) -> Result<(), ParseError> {
    let valid = match (name, parameter) {
        (
            ConditionName::True
            | ConditionName::False
            | ConditionName::RuleApplied
            | ConditionName::RuleEvaluated
            | ConditionName::Random,
            None,
        ) => true,
        (ConditionName::LocalTime, Some((p, _))) => is_time_range(p),
        (ConditionName::RuleApplied | ConditionName::RuleEvaluated, Some((p, _))) => is_duration(p),
        (ConditionName::Random, Some((p, _))) => is_probability(p),
        (ConditionName::AllowedMatches, Some((p, offset))) => {
            // The parameter is a query. Its own errors are reported at the
            // parameter's position: the nested offsets would be relative.
            return parse(p, Mode::Query)
                .map(|_| ())
                .map_err(|_| ParseError::new(ParseErrorKind::InvalidConditionParameter, offset));
        }
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        let offset = parameter.map_or(0, |(_, o)| o);
        Err(ParseError::new(
            ParseErrorKind::InvalidConditionParameter,
            offset,
        ))
    }
}

/// `HH:MM[:SS]` with each part one or two digits.
fn is_time_of_day(text: &[u8]) -> bool {
    let parts: Vec<&[u8]> = text.split(|&b| b == b':').collect();
    let two_digit = |p: &&[u8]| (1..=2).contains(&p.len()) && p.iter().all(u8::is_ascii_digit);
    (2..=3).contains(&parts.len()) && parts.iter().all(two_digit)
}

/// `HH:MM[:SS][-HH:MM[:SS]]`
fn is_time_range(text: &[u8]) -> bool {
    let text = text.trim_ascii();
    match text.iter().position(|&b| b == b'-') {
        None => is_time_of_day(text),
        Some(dash) => {
            let (start, end) = text.split_at(dash);
            is_time_of_day(start) && is_time_of_day(end.get(1..).unwrap_or_default())
        }
    }
}

/// `HH:MM:SS`, `HH:MM`, or `SS` (any number of seconds).
fn is_duration(text: &[u8]) -> bool {
    let text = text.trim_ascii();
    if !text.is_empty() && text.len() <= 10 && text.iter().all(u8::is_ascii_digit) {
        return true;
    }
    is_time_of_day(text)
}

/// A decimal number in `[0, 1]`.
fn is_probability(text: &[u8]) -> bool {
    let text = text.trim_ascii();
    text.len() <= 32
        && text.iter().all(|b| b.is_ascii_digit() || *b == b'.')
        && std::str::from_utf8(text)
            .ok()
            .and_then(|s| s.parse::<f64>().ok())
            .is_some_and(|p| (0.0..=1.0).contains(&p))
}
