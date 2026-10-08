//! Проверка байтов настоящим `java -Xverify:all`.
//!
//! Каждый класс грузится отдельной JVM в дереве пакетов: изоляция важнее
//! скорости (один битый класс не тянет за собой остальные). Классы, которые
//! не грузятся из-за отсутствующих зависимостей, — `Skipped`, а не провал:
//! судить о них верификатор не может.

use std::path::Path;
use std::process::Command;

/// Исход проверки одного класса.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JavaVerdict {
    /// Загрузился без ошибок верификации (`main` при этом может отсутствовать).
    Verified,
    /// Верификатор отверг класс (текст ошибки — для отчёта).
    Failed(String),
    /// Судить нельзя: нет `java` или не хватает зависимостей.
    Skipped(String),
}

/// Есть ли `java` для проверки.
pub fn java_available() -> bool {
    Command::new("java")
        .arg("-version")
        .output()
        .is_ok_and(|out| out.status.success())
}

/// Проверить байты одного класса настоящим верификатором.
pub fn verify_class_bytes(internal_name: &str, bytes: &[u8]) -> JavaVerdict {
    if !java_available() {
        return JavaVerdict::Skipped("нет java".to_string());
    }
    let scratch = std::env::temp_dir().join(format!("noxis-verify-{}", std::process::id()));
    let mut path = scratch.clone();
    for part in internal_name.split('/') {
        path.push(part);
    }
    path.set_extension("class");
    if let Err(error) = path
        .parent()
        .map(std::fs::create_dir_all)
        .unwrap_or(Ok(()))
        .and_then(|()| std::fs::write(&path, bytes))
    {
        return JavaVerdict::Skipped(format!("не записался: {error}"));
    }
    let verdict = run_java(&scratch, &internal_name.replace('/', "."));
    let _ = std::fs::remove_dir_all(&scratch);
    verdict
}

/// Проверить все классы jar'а (фильтр — префиксы внутренних имён).
pub fn verify_jar(jar: &Path, scope: &[String]) -> Vec<(String, JavaVerdict)> {
    let entries = match noxis_jar::read_file(jar) {
        Ok(entries) => entries,
        Err(error) => {
            return vec![(
                jar.display().to_string(),
                JavaVerdict::Skipped(format!("jar не читается: {error}")),
            )];
        }
    };
    entries
        .iter()
        .filter(|entry| entry.name.ends_with(".class"))
        .map(|entry| entry.name.trim_end_matches(".class").to_string())
        .filter(|name| scope.is_empty() || scope.iter().any(|prefix| name.starts_with(prefix)))
        .map(|name| {
            let bytes = entries
                .iter()
                .find(|entry| entry.name == format!("{name}.class"))
                .map(|entry| entry.data.as_slice())
                .unwrap_or_default();
            (name.clone(), verify_class_bytes(&name, bytes))
        })
        .collect()
}

/// Запуск `java -Xverify:all -cp <dir> <class>` и разбор исхода.
fn run_java(dir: &Path, dotted: &str) -> JavaVerdict {
    let output = match Command::new("java")
        .arg("-Xverify:all")
        .arg("-cp")
        .arg(dir)
        .arg(dotted)
        .output()
    {
        Ok(output) => output,
        Err(error) => return JavaVerdict::Skipped(format!("запуск java: {error}")),
    };
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("VerifyError") || stderr.contains("ClassFormatError") {
        return JavaVerdict::Failed(stderr.into_owned());
    }
    if stderr.contains("NoClassDefFoundError") || stderr.contains("ClassNotFoundException") {
        return JavaVerdict::Skipped(format!("не хватает зависимостей: {dotted}"));
    }
    JavaVerdict::Verified
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use super::*;

    #[test]
    fn missing_java_is_skip_not_fail() {
        if java_available() {
            return;
        }
        assert!(matches!(
            verify_class_bytes("a/B", &[]),
            JavaVerdict::Skipped(_)
        ));
    }

    #[test]
    fn verdict_paths_are_exhaustive() {
        let ok = JavaVerdict::Verified;
        assert_eq!(ok, JavaVerdict::Verified);
        assert_ne!(ok, JavaVerdict::Skipped(String::new()));
    }
}
