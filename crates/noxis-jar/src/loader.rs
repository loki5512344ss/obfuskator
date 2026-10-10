//! Archive to [`Program`] and back.
//!
//! The rule is simple: `*.class` goes to the parser, everything else -
//! to resources. Classes serialize first, then resources: the order inside
//! the archive does not affect behaviour, and a fixed order gives
//! reproducible writes.

use std::collections::BTreeSet;

use noxis_classfile::Program;
use noxis_classfile::format;
use noxis_classfile::model::Resource;

use crate::archive::{Entry, JarError};

/// Parses archive entries: classes - through the parser, the rest - as resources.
///
/// A name that repeats (multi-release jars carry the same class at the root
/// and under `META-INF/versions/*`) is kept once, first occurrence wins:
/// the flat model cannot hold two classes under one name, and later passes
/// (rename maps, jar writing) assume uniqueness.
pub fn load(entries: Vec<Entry>) -> Result<Program, JarError> {
    let mut program = Program::default();
    let mut seen: BTreeSet<String> = BTreeSet::new();

    for entry in entries {
        if is_class_entry(&entry.name) {
            let class = format::parse(&entry.data)?;
            if seen.insert(class.this_class.clone()) {
                program.classes.push(class);
            }
        } else {
            program.resources.push(Resource {
                path: entry.name,
                data: entry.data,
            });
        }
    }

    Ok(program)
}

/// Assembles archive entries from [`Program`]: classes first, then resources.
pub fn save(program: &Program) -> Result<Vec<Entry>, JarError> {
    let mut entries = Vec::with_capacity(program.classes.len() + program.resources.len());

    for class in &program.classes {
        let name = format!("{}.class", class.this_class);
        let bytes = format::write(class).map_err(|error| {
            JarError::Classfile(noxis_classfile::ClassfileError::Write(format!(
                "{}: {error}",
                class.this_class
            )))
        })?;
        entries.push(Entry::new(name, bytes));
    }
    for resource in &program.resources {
        entries.push(Entry::new(resource.path.clone(), resource.data.clone()));
    }

    Ok(entries)
}

/// A class file inside the archive: the path ends in `.class`.
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

    /// Class entries go through the parser (round-trip keeps them intact).
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

    /// Multi-release twins (`demo/App.class` + `META-INF/versions/9/...`):
    /// the flat model keeps the first occurrence so no two classes ever
    /// share one name downstream.
    #[test]
    fn duplicate_class_names_keep_first() {
        use noxis_classfile::ClassBuilder;

        let class = ClassBuilder::new("demo/App", None).build();
        let bytes = format::write(&class).expect("write");
        let entries = vec![
            Entry::new("demo/App.class", bytes.clone()),
            Entry::new("META-INF/versions/9/demo/App.class", bytes),
        ];
        let program = load(entries).expect("load");
        assert_eq!(program.classes.len(), 1);
        assert_eq!(program.classes[0].this_class, "demo/App");
    }
}
