//! Renders rules back to text, with correct quoting and escaping.
//!
//! Escaping is a security property here, not cosmetics: an attacker-supplied
//! device name that could close a quote would become a rule fragment with its
//! own meaning (SECURITY.md, "Rule-text injection").

use std::fmt::{self, Write as _};

use crate::model::{
    Attribute, AttributeValue, AttributeValues, Condition, ConditionClause, ParseError,
    ParseErrorKind, Rule, RuleString,
};

use super::parser::{Mode, parse};

impl fmt::Display for Rule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;
        let mut sep = |f: &mut fmt::Formatter<'_>| {
            if first {
                first = false;
                Ok(())
            } else {
                f.write_char(' ')
            }
        };
        if let Some(target) = self.target {
            sep(f)?;
            f.write_str(target.keyword())?;
        }
        for attribute in &self.attributes {
            sep(f)?;
            write!(f, "{attribute}")?;
        }
        if let Some(condition) = &self.condition {
            sep(f)?;
            write!(f, "if {condition}")?;
        }
        if let Some(comment) = &self.comment {
            sep(f)?;
            write!(f, "#{}", String::from_utf8_lossy(comment))?;
        }
        Ok(())
    }
}

impl fmt::Display for Attribute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name.keyword())?;
        match &self.values {
            AttributeValues::Single(v) => write!(f, " {v}"),
            AttributeValues::Set { operator, values } => {
                if let Some(op) = operator {
                    write!(f, " {}", op.keyword())?;
                }
                f.write_str(" {")?;
                for v in values {
                    write!(f, " {v}")?;
                }
                f.write_str(" }")
            }
        }
    }
}

impl fmt::Display for AttributeValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UsbId(id) => write!(f, "{id}"),
            Self::Interface(i) => write!(f, "{i}"),
            Self::String(s) => write_quoted(f, s),
        }
    }
}

impl fmt::Display for ConditionClause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Single(c) => write!(f, "{c}"),
            Self::Set {
                operator,
                conditions,
            } => {
                if let Some(op) = operator {
                    write!(f, "{} ", op.keyword())?;
                }
                f.write_char('{')?;
                for c in conditions {
                    write!(f, " {c}")?;
                }
                f.write_str(" }")
            }
        }
    }
}

impl fmt::Display for Condition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.negated {
            f.write_char('!')?;
        }
        f.write_str(self.name.keyword())?;
        if let Some(p) = &self.parameter {
            // Parameters are kept verbatim from parsed text; they were valid
            // there, so they are valid here.
            write!(f, "({})", String::from_utf8_lossy(p))?;
        }
        Ok(())
    }
}

/// Writes `s` as a quoted string. `"` and `\` are escaped; so is every byte
/// that is a control character or not part of valid UTF-8, as `\xHH`.
fn write_quoted(f: &mut fmt::Formatter<'_>, s: &RuleString) -> fmt::Result {
    f.write_char('"')?;
    for chunk in s.as_bytes().utf8_chunks() {
        for c in chunk.valid().chars() {
            match c {
                '"' => f.write_str("\\\"")?,
                '\\' => f.write_str("\\\\")?,
                c if c.is_control() => {
                    // Every control char is either ASCII or U+0080..U+009F;
                    // escape each of its UTF-8 bytes.
                    let mut buf = [0u8; 4];
                    for b in c.encode_utf8(&mut buf).bytes() {
                        write!(f, "\\x{b:02x}")?;
                    }
                }
                c => f.write_char(c)?,
            }
        }
        for b in chunk.invalid() {
            write!(f, "\\x{b:02x}")?;
        }
    }
    f.write_char('"')
}

/// Renders a rule and verifies that the text parses back to the same rule
/// (§7.2): a generator that produces text the parser rejects is a bug caught
/// here instead of by the daemon.
///
/// # Errors
///
/// Returns the parse error if the rendered text does not parse, or
/// [`ParseErrorKind::InvalidValue`] if it parses to a different rule.
pub fn render_checked(rule: &Rule) -> Result<String, ParseError> {
    let text = rule.to_string();
    let reparsed = parse(text.as_bytes(), Mode::Query)?;
    if &reparsed == rule {
        Ok(text)
    } else {
        Err(ParseError::new(ParseErrorKind::InvalidValue, 0))
    }
}
