use super::{TokenKind, tokenize};
use crate::model::ParseErrorKind;

fn kinds(input: &str) -> Vec<TokenKind<'_>> {
    tokenize(input.as_bytes())
        .unwrap()
        .into_iter()
        .map(|t| t.kind)
        .collect()
}

#[test]
fn words_braces_and_strings() {
    assert_eq!(
        kinds(r#"allow with-interface { 03:01:01 } name "a b""#),
        vec![
            TokenKind::Word(b"allow"),
            TokenKind::Word(b"with-interface"),
            TokenKind::LBrace,
            TokenKind::Word(b"03:01:01"),
            TokenKind::RBrace,
            TokenKind::Word(b"name"),
            TokenKind::Str(b"a b".to_vec()),
        ]
    );
}

#[test]
fn escapes_including_non_utf8() {
    assert_eq!(
        kinds(r#""q\"b\\s\xff\t""#),
        vec![TokenKind::Str(b"q\"b\\s\xff\t".to_vec())]
    );
}

#[test]
fn condition_parameters_keep_nested_parens_and_strings() {
    assert_eq!(
        kinds(r#"if !allowed-matches(name "x)" if rule-applied(10))"#),
        vec![
            TokenKind::Word(b"if"),
            TokenKind::Bang,
            TokenKind::Word(b"allowed-matches"),
            TokenKind::Param(br#"name "x)" if rule-applied(10)"#),
        ]
    );
}

#[test]
fn comment_runs_to_the_end() {
    assert_eq!(
        kinds("block # a { b"),
        vec![TokenKind::Word(b"block"), TokenKind::Comment(b" a { b")]
    );
}

#[test]
fn errors_have_offsets() {
    let err = tokenize(br#"name "abc"#).unwrap_err();
    assert_eq!(
        (err.kind, err.offset),
        (ParseErrorKind::UnterminatedString, 5)
    );
    let err = tokenize(br#"name "\q""#).unwrap_err();
    assert_eq!((err.kind, err.offset), (ParseErrorKind::InvalidEscape, 6));
    let err = tokenize(b"if random(0.5").unwrap_err();
    assert_eq!(err.kind, ParseErrorKind::UnterminatedParameter);
    let err = tokenize(b"x((((((((((1))))))))))").unwrap_err();
    assert_eq!(err.kind, ParseErrorKind::TooDeep);
}

#[test]
fn every_control_escape_keeps_its_byte_value() {
    assert_eq!(
        kinds(r#""\a\b\f\n\r\t\v\'\?""#),
        vec![TokenKind::Str(vec![7, 8, 12, 10, 13, 9, 11, b'\'', b'?'])]
    );
    for bytes in [&b""[..], b"1", b"123", b"gg", b"0g", b"g0"] {
        assert_eq!(super::parse_hex_byte(bytes), None);
    }
    assert_eq!(super::parse_hex_byte(b"aF"), Some(175));
}
