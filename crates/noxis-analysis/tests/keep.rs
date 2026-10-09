//! Keep-rule unit tests: globs and exact names.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use noxis_analysis::KeepRules;
use noxis_core::Config;

fn rules_with_patterns() -> KeepRules {
    KeepRules::from_config(&Config {
        keep: vec!["com/example/*".to_string(), "**/internal/**".to_string()],
        keep_resources: vec!["META-INF/services/*".to_string()],
        ..Config::default()
    })
    .expect("patterns")
}

#[test]
fn glob_semantics() {
    let rules = rules_with_patterns();
    assert!(rules.keeps_class("com/example/Main"));
    assert!(!rules.keeps_class("com/example/sub/Main"));
    assert!(rules.keeps_class("x/internal/y/Z"));
    assert!(rules.keeps_resource("META-INF/services/a.b.C"));
    assert!(!rules.keeps_resource("assets/mc/mod.json"));
}

#[test]
fn exact_names_win() {
    let mut rules = KeepRules::default();
    rules.keep_exact("app/Main");
    assert!(rules.keeps_class("app/Main"));
    assert!(!rules.keeps_class("app/Other"));
}
