//! Перевод архива в [`Program`] и обратно.
//!
//! Правило простое: `*.class` уходит в парсер, всё остальное — в ресурсы.
//! Обратно классы сериализуются первыми, затем ресурсы: порядок внутри
//! архива на поведение не влияет, а фиксированный порядок даёт
//! воспроизводимую запись.

use noxis_classfile::Program;
use noxis_classfile::format;
use noxis_classfile::model::Resource;

use crate::archive::{Entry, JarError};

/// Разбирает записи архива: классы — через парсер, остальное — как ресурсы.
pub fn load(entries: Vec<Entry>) -> Result<Program, JarError> {
    let mut program = Program::default();

    for entry in entries {
        if is_class_entry(&entry.name) {
            program.classes.push(format::parse(&entry.data)?);
        } else {
            program.resources.push(Resource {
                path: entry.name,
                data: entry.data,
            });
        }
    }

    Ok(program)
}

/// Собирает записи архива из [`Program`]: сначала классы, потом ресурсы.
pub fn save(program: &Program) -> Result<Vec<Entry>, JarError> {
    let mut entries = Vec::with_capacity(program.classes.len() + program.resources.len());

    for class in &program.classes {
        let name = format!("{}.class", class.this_class);
        entries.push(Entry::new(name, format::write(class)?));
    }
    for resource in &program.resources {
        entries.push(Entry::new(resource.path.clone(), resource.data.clone()));
    }

    Ok(entries)
}

/// Классовый файл внутри архива: путь оканчивается на `.class`.
fn is_class_entry(name: &str) -> bool {
    name.len() > ".class".len() && name.ends_with(".class")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use super::*;

    #[test]
    fn class_paths_are_recognised() {
        assert!(is_class_entry("demo/App.class"));
        assert!(is_class_entry("a/b/C.class"));
        assert!(!is_class_entry(".class"));
        assert!(!is_class_entry("classes.txt"));
        assert!(!is_class_entry("demo/App.class.bak"));
    }

    #[test]
    fn resources_round_trip_without_classes() {
        let source = vec![
            Entry::new("META-INF/MANIFEST.MF", b"Manifest-Version: 1.0\n".to_vec()),
            Entry::new("config/app.json", br#"{"k":1}"#.to_vec()),
        ];

        let program = load(source.clone()).expect("load");
        assert!(program.classes.is_empty());
        assert_eq!(program.resources.len(), 2);

        assert_eq!(save(&program).expect("save"), source);
    }

    /// До P1 `format::parse` — заглушка; после P1 ожидается успешный разбор.
    #[test]
    fn class_entries_go_through_the_parser() {
        let entries = vec![
            Entry::new("demo/App.class", vec![0xCA, 0xFE]),
            Entry::new("notes.txt", b"hi".to_vec()),
        ];

        match load(entries) {
            Ok(program) => {
                assert_eq!(program.classes.len(), 1);
                assert_eq!(program.resources.len(), 1);
            }
            Err(JarError::Classfile(err)) => {
                assert!(!err.to_string().is_empty());
            }
            Err(other) => panic!("unexpected error: {other}"),
        }
    }
}
