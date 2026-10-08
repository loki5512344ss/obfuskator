//! Обратная таблица маппингов: деобфускация имён и stack-trace translate.

use std::collections::BTreeMap;

use serde::Serialize;

/// Ключ метода: `owner/name+descriptor`.
pub fn method_key(owner: &str, name: &str, descriptor: &str) -> String {
    format!("{owner}/{name}{descriptor}")
}

/// Ключ поля: `owner/name:descriptor`.
pub fn field_key(owner: &str, name: &str, descriptor: &str) -> String {
    format!("{owner}/{name}:{descriptor}")
}

/// Таблицы «оригинал → обфусцированное».
///
/// Нужна по двум причинам: (1) перевод stack trace'ей обратно в читаемые имена
/// (аналог ZKM Stack Trace Translate), (2) аудит — что именно поменялось.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Mapping {
    /// internal name класса → новый internal name
    pub classes: BTreeMap<String, String>,
    /// `owner/name+desc` → новое имя метода (владелец остаётся прежним по ключу)
    pub methods: BTreeMap<String, String>,
    /// `owner/name:desc` → новое имя поля
    pub fields: BTreeMap<String, String>,
    /// имя локальной переменной/параметра в методе → новое (по номеру слота)
    pub locals: BTreeMap<String, String>,
    /// оригинальный путь ресурса → новый
    pub resources: BTreeMap<String, String>,
}

impl Mapping {
    /// Новое имя класса, если он переименован.
    pub fn class(&self, from: &str) -> Option<&str> {
        self.classes.get(from).map(String::as_str)
    }

    /// Новое имя метода.
    pub fn method(&self, owner: &str, name: &str, descriptor: &str) -> Option<&str> {
        self.methods
            .get(&method_key(owner, name, descriptor))
            .map(String::as_str)
    }

    /// Новое имя поля.
    pub fn field(&self, owner: &str, name: &str, descriptor: &str) -> Option<&str> {
        self.fields
            .get(&field_key(owner, name, descriptor))
            .map(String::as_str)
    }

    /// Число переименований всех видов.
    pub fn len(&self) -> usize {
        self.classes.len() + self.methods.len() + self.fields.len() + self.locals.len()
    }

    /// Пустая ли таблица.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Человекочитаемая таблица для файла маппингов и deobfuscator'а.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        out.push_str("# classes\n");
        for (from, to) in &self.classes {
            out.push_str(&format!("{from} -> {to}\n"));
        }
        out.push_str("\n# methods\n");
        for (from, to) in &self.methods {
            out.push_str(&format!("{from} -> {to}\n"));
        }
        out.push_str("\n# fields\n");
        for (from, to) in &self.fields {
            out.push_str(&format!("{from} -> {to}\n"));
        }
        out.push_str("\n# resources\n");
        for (from, to) in &self.resources {
            out.push_str(&format!("{from} -> {to}\n"));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]
    use super::*;

    #[test]
    fn roundtrip_lookup() {
        let mut m = Mapping::default();
        m.classes.insert("com/example/Main".into(), "a/B".into());
        m.methods
            .insert(method_key("a/B", "run", "()V"), "x".into());
        assert_eq!(m.class("com/example/Main"), Some("a/B"));
        assert_eq!(m.method("a/B", "run", "()V"), Some("x"));
        assert!(m.method("a/B", "other", "()V").is_none());
    }

    #[test]
    fn text_report_contains_sections() {
        let mut m = Mapping::default();
        m.classes.insert("A".into(), "B".into());
        let text = m.to_text();
        assert!(text.contains("# classes"));
        assert!(text.contains("A -> B"));
    }
}
