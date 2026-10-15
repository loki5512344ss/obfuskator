//! Tests for TOML-subset → `Config` struct population.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::*;
use crate::config::sections::NameStyle;
use crate::config::toml_lite;
use crate::config::{PassId, Profile};

fn build_from(text: &str) -> Res<Config> {
    let table = toml_lite::parse(text).map_err(|e| e.to_string())?;
    build(&table)
}

#[test]
fn builds_defaults_from_empty_input() {
    let cfg = build_from("").unwrap();
    assert_eq!(cfg, Config::default());
}

#[test]
fn overrides_top_level_and_section_fields() {
    let cfg = build_from(
        r#"
        profile = "paranoid"
        seed = 123
        keep = ["com/example/Main"]
        [rename]
        package_pool = 4
        "#,
    )
    .unwrap();
    assert_eq!(cfg.profile, Profile::Paranoid);
    assert_eq!(cfg.seed, Some(123));
    assert_eq!(cfg.keep, vec!["com/example/Main".to_string()]);
    assert_eq!(cfg.rename.package_pool, 4);
}

#[test]
fn rejects_unknown_top_level_key() {
    assert!(build_from("totally_unknown = 1").is_err());
}

#[test]
fn rejects_unknown_section_key() {
    assert!(build_from("[rename]\nbogus = 1\n").is_err());
}

#[test]
fn parses_enable_disable_pass_lists() {
    let cfg = build_from(
        r#"enable = ["flow"]
disable = ["strings"]
"#,
    )
    .unwrap();
    assert_eq!(cfg.enable, vec![PassId::Flow]);
    assert_eq!(cfg.disable, vec![PassId::Strings]);
}

#[test]
fn parses_name_style_in_rename_section() {
    let cfg = build_from("[rename]\nclass_style = \"il-o0\"\n").unwrap();
    assert_eq!(cfg.rename.class_style, NameStyle::IlO0);
}
