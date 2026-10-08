//! Приёмка `compute_frames` на фикстурах (`tests/data`).
//!
//! Кадры сносятся и считаются заново. Судьи: детерминизм + round-trip IR +
//! настоящий `java -Xverify:all` для классов без внешних зависимостей.
//! (`verify_class` backend'а для приёмки не годится: он ждёт кадры в байтах,
//! а `from_bytes` отдаёт в индексах — см. `todo.md`.)

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use noxis_classfile::{Hierarchy, compute_frames, format};

/// Карта `класс → суперкласс` для LUB.
#[derive(Default)]
struct MapHierarchy {
    supers: BTreeMap<String, String>,
}

impl Hierarchy for MapHierarchy {
    fn superclass(&self, class: &str) -> Option<String> {
        self.supers.get(class).cloned()
    }
}

/// Классы только на JDK — их `java` проверит полностью.
const JDK_ONLY: [&str; 2] = [
    "dev/loki/lorep/util/TimeUtil",
    "dev/loki/lorep/util/PaginationUtil",
];

/// Фикстуры: снести кадры, посчитать заново, проверить выход.
#[test]
fn fixtures_recompute_and_verify() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    let mut names: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("tests/data")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "class"))
        .collect();
    names.sort();
    assert!(!names.is_empty(), "нет фикстур в tests/data");

    let mut supers = BTreeMap::new();
    let mut parsed = Vec::new();
    for name in &names {
        let class = format::parse(&std::fs::read(name).expect("read")).expect("parse");
        supers.insert(class.this_class.clone(), class.super_class.clone());
        parsed.push(class);
    }
    // Без суперкласса — только `java/lang/Object`.
    supers.entry("java/lang/Object".to_string()).or_insert(None);
    let hierarchy = MapHierarchy {
        supers: supers
            .into_iter()
            .filter_map(|(k, v)| v.map(|sup| (k, sup)))
            .collect(),
    };

    let scratch = std::env::temp_dir().join(format!("noxis-frames-{}", std::process::id()));
    let java = java_available();
    for class in &mut parsed {
        if class.version.major < 50 {
            continue;
        }
        for method in &mut class.methods {
            if method.code.is_none() {
                continue;
            }
            let first = compute_frames(&class.this_class, method, &hierarchy).expect("recompute");
            method.code.as_mut().expect("code").frames = first.clone();
            // Детерминизм: повторный прогон даёт те же кадры.
            let second = compute_frames(&class.this_class, method, &hierarchy).expect("recompute");
            assert_eq!(first, second, "недетерминированные кадры");
        }
        // Round-trip: записанное читается в тот же IR (включая кадры).
        let written = format::write(class).expect("write");
        assert_eq!(
            format::parse(&written).expect("re-parse"),
            *class,
            "round-trip"
        );
        if java && JDK_ONLY.contains(&class.this_class.as_str()) {
            java_verify(&scratch, class, &written).expect("java -Xverify:all");
        }
    }
    let _ = std::fs::remove_dir_all(&scratch);
}

/// Есть ли `java` для проверки настоящим верификатором.
fn java_available() -> bool {
    Command::new("java")
        .arg("-version")
        .output()
        .is_ok_and(|out| out.status.success())
}

/// Записать класс в дерево пакетов и прогнать `java -Xverify:all`.
/// Успех — загрузка без ошибок верификации (`main` при этом отсутствует).
fn java_verify(
    scratch: &Path,
    class: &noxis_classfile::ClassFile,
    bytes: &[u8],
) -> Result<(), String> {
    let mut path = scratch.to_path_buf();
    for part in class.this_class.split('/') {
        path.push(part);
    }
    path.set_extension("class");
    std::fs::create_dir_all(path.parent().expect("parent")).map_err(|e| format!("mkdir: {e}"))?;
    std::fs::write(&path, bytes).map_err(|e| format!("write: {e}"))?;
    let dotted = class.this_class.replace('/', ".");
    let out = Command::new("java")
        .arg("-Xverify:all")
        .arg("-cp")
        .arg(scratch)
        .arg(&dotted)
        .output()
        .map_err(|e| format!("запуск java: {e}"))?;
    let stderr = String::from_utf8_lossy(&out.stderr);
    for bad in [
        "VerifyError",
        "ClassFormatError",
        "NoClassDefFoundError",
        "LinkageError",
    ] {
        if stderr.contains(bad) {
            return Err(format!("{dotted}:\n{stderr}"));
        }
    }
    Ok(())
}
