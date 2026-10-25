//! Splits rule text into tokens.
//!
//! Operates on bytes, never on `char`s: a quoted string may carry `\xHH`
//! escapes for bytes that are not UTF-8, and the lexer must not care.

use crate::model::{ParseError, ParseErrorKind};

/// Deepest nesting of parentheses accepted inside a condition parameter.
/// `allowed-matches(…)` contains a query, which may itself contain conditions;
/// the bound is checked while scanning, before any recursion happens.
pub(crate) const MAX_PAREN_DEPTH: usize = 8;

/// One lexical unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TokenKind<'a> {
    /// A bare word: keyword, `vendor:product`, `cc:ss:pp`, …
    Word(&'a [u8]),
    /// A quoted string, already unescaped.
    Str(Vec<u8>),
    /// `{`
    LBrace,
    /// `}`
    RBrace,
    /// `!`
    Bang,
    /// The raw text between a `(` and its matching `)`.
    Param(&'a [u8]),
    /// Everything after `#`, to the end of the input.
    Comment(&'a [u8]),
}

/// A token and the byte offset where it starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Token<'a> {
    pub(crate) kind: TokenKind<'a>,
    pub(crate) offset: usize,
}

/// Bytes that end a bare word.
const fn is_delimiter(b: u8) -> bool {
    b.is_ascii_whitespace() || matches!(b, b'{' | b'}' | b'(' | b')' | b'!' | b'"' | b'#')
}

/// Tokenizes the whole input.
pub(crate) fn tokenize(input: &[u8]) -> Result<Vec<Token<'_>>, ParseError> {
    let mut tokens = Vec::new();
    let mut pos = 0;

    while let Some(&b) = input.get(pos) {
        let start = pos;
        let kind = match b {
            _ if b.is_ascii_whitespace() => {
                pos += 1;
                continue;
            }
            b'{' => {
                pos += 1;
                TokenKind::LBrace
            }
            b'}' => {
                pos += 1;
                TokenKind::RBrace
            }
            b'!' => {
                pos += 1;
                TokenKind::Bang
            }
            b'#' => {
                pos = input.len();
                TokenKind::Comment(input.get(start + 1..).unwrap_or_default())
            }
            b'"' => {
                let (value, end) = lex_string(input, start)?;
                pos = end;
                TokenKind::Str(value)
            }
            b'(' => {
                let end = find_closing_paren(input, start)?;
                pos = end + 1;
                TokenKind::Param(input.get(start + 1..end).unwrap_or_default())
            }
            b')' => return Err(ParseError::new(ParseErrorKind::UnexpectedToken, start)),
            _ => {
                while input.get(pos).is_some_and(|&c| !is_delimiter(c)) {
                    pos += 1;
                }
                TokenKind::Word(input.get(start..pos).unwrap_or_default())
            }
        };
        tokens.push(Token {
            kind,
            offset: start,
        });
    }
    Ok(tokens)
}

/// Lexes a quoted string starting at `start` (which holds `"`). Returns the
/// unescaped bytes and the offset just past the closing quote.
fn lex_string(input: &[u8], start: usize) -> Result<(Vec<u8>, usize), ParseError> {
    let mut out = Vec::new();
    let mut pos = start + 1;
    loop {
        let Some(&b) = input.get(pos) else {
            return Err(ParseError::new(ParseErrorKind::UnterminatedString, start));
        };
        match b {
            b'"' => return Ok((out, pos + 1)),
            b'\\' => {
                let escape_at = pos;
                let Some(&e) = input.get(pos + 1) else {
                    return Err(ParseError::new(ParseErrorKind::UnterminatedString, start));
                };
                pos += 2;
                let byte = match e {
                    b'"' | b'\\' | b'\'' | b'?' => e,
                    b'a' => 0x07,
                    b'b' => 0x08,
                    b'f' => 0x0c,
                    b'n' => b'\n',
                    b'r' => b'\r',
                    b't' => b'\t',
                    b'v' => 0x0b,
                    b'x' => {
                        let hex = input
                            .get(pos..pos + 2)
                            .and_then(parse_hex_byte)
                            .ok_or(ParseError::new(ParseErrorKind::InvalidEscape, escape_at))?;
                        pos += 2;
                        hex
                    }
                    _ => return Err(ParseError::new(ParseErrorKind::InvalidEscape, escape_at)),
                };
                out.push(byte);
            }
            _ => {
                out.push(b);
                pos += 1;
            }
        }
    }
}

fn parse_hex_byte(two: &[u8]) -> Option<u8> {
    let [hi, lo] = two else { return None };
    let hi = char::from(*hi).to_digit(16)?;
    let lo = char::from(*lo).to_digit(16)?;
    u8::try_from(hi << 4 | lo).ok()
}

/// Finds the `)` matching the `(` at `open`, skipping quoted strings and
/// counting nesting. Fails beyond [`MAX_PAREN_DEPTH`].
fn find_closing_paren(input: &[u8], open: usize) -> Result<usize, ParseError> {
    let mut depth = 0usize;
    let mut pos = open;
    while let Some(&b) = input.get(pos) {
        match b {
            b'(' => {
                depth += 1;
                if depth > MAX_PAREN_DEPTH {
                    return Err(ParseError::new(ParseErrorKind::TooDeep, pos));
                }
            }
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Ok(pos);
                }
            }
            b'"' => {
                // Reuse the string lexer only to find where the string ends.
                let (_, end) = lex_string(input, pos)?;
                pos = end;
                continue;
            }
            _ => {}
        }
        pos += 1;
    }
    Err(ParseError::new(ParseErrorKind::UnterminatedParameter, open))
}

#[cfg(test)]
#[path = "../../tests/unit/rules_lexer.rs"]
mod tests;
