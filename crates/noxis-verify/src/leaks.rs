//! Утечки имён и секреты открытым текстом.
//!
//! Два независимых вопроса: (1) пережило ли исходное имя обфускацию
//! (сверка эталона до/после с учётом keep); (2) остались ли в строках
//! секреты, которые обязан был съесть пасс `strings`.

use std::collections::BTreeSet;

use noxis_analysis::KeepRules;
use noxis_classfile::{Const, Insn, Program};

/// Красный список: паттерны секретов, которых не должно быть в выходе.
///
/// Это generic-признаки (пароли, ключи, URL, приватные ключи), а не имена
/// конкретного проекта: проектные секреты ловятся тем, что строка вообще
/// осталась незашифрованной (см. `scan_secrets` + keep-список пасса).
pub const RED_PATTERNS: &[&str] = &[
    r"(?i)password\s*=\s*[^\s;]+",
    r"(?i)\bapi[_-]?key\b",
    r#"jdbc:[a-z]+://[^\s"']+"#,
    r"-----BEGIN [A-Z ]*PRIVATE KEY-----",
    r#"https?://[^\s"']+"#,
];

/// Все идентификаторы программы: классы, члены, строки кода.
///
/// Члены - по простому имени (без владельца): сверка строгая и дешёвая,
/// для точечной проверки есть `find_leaks` с keep-фильтром.
pub fn collect_names(program: &Program) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for class in &program.classes {
        names.insert(class.this_class.clone());
        for field in &class.fields {
            names.insert(field.name.clone());
        }
        for method in &class.methods {
            names.insert(method.name.clone());
        }
    }
    names
}

/// Имена из эталона, пережившие обфускацию (минус keep).
///
/// Пустой результат - норма; непустой - тест падает с этим списком.
pub fn find_leaks(before: &BTreeSet<String>, after: &Program, keep: &KeepRules) -> Vec<String> {
    let names = collect_names(after);
    before
        .iter()
        .filter(|name| names.contains(*name))
        .filter(|name| !keep.keeps_class(name) && !keep.keeps_member(name))
        .cloned()
        .collect()
}

/// Какие красные паттерны сработали на тексте.
pub fn check_patterns(text: &str) -> Vec<&'static str> {
    use std::sync::OnceLock;

    static COMPILED: OnceLock<Vec<(&'static str, regex::Regex)>> = OnceLock::new();
    let compiled = COMPILED.get_or_init(|| {
        RED_PATTERNS
            .iter()
            .filter_map(|pattern| {
                regex::Regex::new(pattern)
                    .ok()
                    .map(|regex| (*pattern, regex))
            })
            .collect()
    });
    compiled
        .iter()
        .filter(|(_, regex)| regex.is_match(text))
        .map(|(pattern, _)| *pattern)
        .collect()
}

/// Секрет открытым текстом: где (`класс/метод` или `ресурс:путь`) и что.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretHit {
    pub location: String,
    pub pattern: &'static str,
    pub sample: String,
}

/// Все строки кода и текстовых ресурсов против красного списка.
pub fn scan_secrets(program: &Program) -> Vec<SecretHit> {
    let mut hits = Vec::new();
    for class in &program.classes {
        for method in &class.methods {
            let Some(code) = method.code.as_ref() else {
                continue;
            };
            for insn in &code.insns {
                if let Insn::Const(Const::String(text)) = insn {
                    let location = format!("{}/{}", class.this_class, method.name);
                    hits.extend(check_text(&location, text));
                }
            }
        }
    }
    for resource in &program.resources {
        if let Ok(text) = std::str::from_utf8(&resource.data) {
            hits.extend(check_text(&format!("ресурс:{}", resource.path), text));
        }
    }
    hits
}

fn check_text(location: &str, text: &str) -> Vec<SecretHit> {
    check_patterns(text)
        .into_iter()
        .map(|pattern| SecretHit {
            location: location.to_string(),
            pattern,
            sample: text.chars().take(48).collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use super::*;

    #[test]
    fn red_patterns_all_compile() {
        assert_eq!(check_patterns("password=hunter2").len(), 1);
        assert_eq!(RED_PATTERNS.len(), 5);
    }

    #[test]
    fn red_patterns_catch_obvious_secrets() {
        assert!(!check_patterns("password=hunter2").is_empty());
        assert!(!check_patterns("api_key=abc").is_empty());
        assert!(!check_patterns("jdbc:mysql://host/db").is_empty());
        assert!(!check_patterns("https://example.com/hook").is_empty());
    }

    #[test]
    fn clean_text_passes() {
        assert!(check_patterns("hello world").is_empty());
        assert!(check_patterns("aB3_foO").is_empty());
    }
}
