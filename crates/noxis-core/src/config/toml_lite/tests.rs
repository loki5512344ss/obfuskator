//! Tests for the TOML-subset parser.
#![allow(clippy::expect_used, clippy::unwrap_used)]
use super::*;

#[test]
fn parses_flat_pairs() {
    let t = parse("name = \"x\"\ncount = 4\nflag = true\n").unwrap();
    assert_eq!(t.get("name"), Some(&Value::String("x".into())));
    assert_eq!(t.get("count"), Some(&Value::Integer(4)));
    assert_eq!(t.get("flag"), Some(&Value::Bool(true)));
}

#[test]
fn parses_negative_integer() {
    let t = parse("x = -12").unwrap();
    assert_eq!(t.get("x"), Some(&Value::Integer(-12)));
}

#[test]
fn parses_section_header() {
    let t = parse("[rename]\npackage_pool = 4\n").unwrap();
    let Some(Value::Table(section)) = t.get("rename") else {
        panic!("expected table");
    };
    assert_eq!(section.get("package_pool"), Some(&Value::Integer(4)));
}

#[test]
fn parses_nested_section_header() {
    let t = parse("[a.b]\nx = 1\n").unwrap();
    let Some(Value::Table(a)) = t.get("a") else {
        panic!("expected table a")
    };
    let Some(Value::Table(b)) = a.get("b") else {
        panic!("expected table b")
    };
    assert_eq!(b.get("x"), Some(&Value::Integer(1)));
}

#[test]
fn parses_string_array() {
    let t = parse(r#"keep = ["com/example/Main", "*Service"]"#).unwrap();
    assert_eq!(
        t.get("keep"),
        Some(&Value::Array(vec![
            Value::String("com/example/Main".into()),
            Value::String("*Service".into()),
        ]))
    );
}

#[test]
fn parses_empty_array() {
    let t = parse("keep = []").unwrap();
    assert_eq!(t.get("keep"), Some(&Value::Array(vec![])));
}

#[test]
fn skips_comments_and_blank_lines() {
    let t = parse("# comment\n\nx = 1 # trailing\n").unwrap();
    assert_eq!(t.get("x"), Some(&Value::Integer(1)));
}

#[test]
fn hash_inside_string_is_not_a_comment() {
    let t = parse(r#"x = "a#b""#).unwrap();
    assert_eq!(t.get("x"), Some(&Value::String("a#b".into())));
}

#[test]
fn rejects_float() {
    assert!(parse("x = 1.5").is_err());
}

#[test]
fn rejects_garbage_line() {
    assert!(parse("not a key value line").is_err());
}

#[test]
fn rejects_unterminated_string() {
    assert!(parse("x = \"unterminated").is_err());
}
