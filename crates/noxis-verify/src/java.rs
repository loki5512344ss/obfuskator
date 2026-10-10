//! Проверка байтов настоящим `java -Xverify:all`.
//!
//! Каждый класс грузится отдельной JVM в дереве пакетов: изоляция важнее
//! скорости (один битый класс не тянет за собой остальные). Классы, которые
//! не грузятся из-за отсутствующих зависимостей, - `Skipped`, а не провал:
//! судить о них верификатор не может.

use std::path::Path;
use std::process::Command;

/// Исход проверки одного класса.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JavaVerdict {
    /// Загрузился без ошибок верификации (`main` при этом может отсутствовать).
    Verified,
    /// Верификатор отверг класс (текст ошибки - для отчёта).
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

/// Whether `jadx` is available for decompile checks.
pub fn jadx_available() -> bool {
    Command::new("jadx")
        .arg("--version")
        .output()
        .is_ok_and(|out| out.status.success())
}

/// Decompile quality of one jar: how much of the output jadx itself flags.
///
/// `JADX WARN` markers mean recovered-but-dubious structures; `Failed`
/// markers mean unrecovered ones. Obfuscation wins when these climb while
/// `java -Xverify:all` stays green: the bytecode runs, but readers drown.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DecompileReport {
    /// `.java` files emitted.
    pub files: usize,
    /// Files with at least one `JADX WARN`.
    pub files_with_warn: usize,
    /// Total `JADX WARN` markers.
    pub warns: usize,
    /// `JADX WARN: Failed...` markers (unrecovered structures).
    pub failed: usize,
    /// Warn markers by kind (first word after the prefix).
    pub kinds: std::collections::BTreeMap<String, usize>,
    /// jadx exit code.
    pub exit_code: Option<i32>,
}

/// Decompile a jar with jadx into a scratch dir and grade the output.
/// `None` when jadx is missing or refuses to start.
pub fn decompile_report(jar: &Path) -> Option<DecompileReport> {
    if !jadx_available() {
        return None;
    }
    let scratch = std::env::temp_dir().join(format!("noxis-jadx-rep-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    let output = Command::new("jadx")
        .arg("-q")
        .arg("-d")
        .arg(&scratch)
        .arg(jar)
        .output()
        .ok()?;
    let mut report = DecompileReport {
        exit_code: output.status.code(),
        ..DecompileReport::default()
    };
    let mut stack = vec![scratch.clone()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "java") {
                report.files += 1;
                let text = std::fs::read(&path)
                    .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
                    .unwrap_or_default();
                let mut file_warns = 0;
                for line in text.lines() {
                    if let Some(rest) = line.find("JADX WARN:").map(|i| &line[i + 10..]) {
                        let kind: String = rest
                            .trim_start_matches([' ', '\'', '"'])
                            .split_whitespace()
                            .next()
                            .unwrap_or("?")
                            .trim_matches(['\'', '"', ':'])
                            .to_string();
                        *report.kinds.entry(kind.clone()).or_insert(0) += 1;
                        if kind == "Failed" {
                            report.failed += 1;
                        }
                        report.warns += 1;
                        file_warns += 1;
                    }
                }
                if file_warns > 0 {
                    report.files_with_warn += 1;
                }
            }
        }
    }
    let _ = std::fs::remove_dir_all(&scratch);
    Some(report)
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

/// Проверить все классы jar'а (фильтр - префиксы внутренних имён).
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
