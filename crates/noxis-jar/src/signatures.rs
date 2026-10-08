//! Срез невалидных подписей JAR.
//!
//! После перепаковки архива `META-INF/*.SF` и `*.RSA|DSA|EC` описывают
//! **старые** байты: JVM либо отказывается запускать jar, либо ругается на
//! подпись. Вырезать их обязательно — и это делается на уровне записей,
//! потому что `MANIFEST.MF` остаётся, а его секции `Name:` тоже нужно
//! выкинуть (ими занимается пасс `resources`).

use crate::archive::Entry;

/// Расширения файлов подписи в `META-INF/` (без учёта регистра).
const SIGNATURE_EXTS: [&str; 4] = [".SF", ".RSA", ".DSA", ".EC"];

/// Префикс блочных файлов подписи старого образца (`META-INF/SIG-*`).
const LEGACY_PREFIX: &str = "SIG-";

/// Путь каталога подписей.
const META_INF: &str = "META-INF/";

/// Проверяет, является ли запись архива файлом подписи.
///
/// Совпадение — только для файлов, лежащих **непосредственно** в
/// `META-INF/`: вложенный `META-INF/certs/x.SF` — обычный ресурс.
///
/// Регистр не важен (`meta-inf/x.sf` — тоже подпись), `MANIFEST.MF`
/// никогда не является подписью.
pub fn is_signature_entry(name: &str) -> bool {
    let Some(relative) = name.strip_prefix(META_INF) else {
        return false;
    };
    if relative.is_empty() || relative.contains('/') {
        return false;
    }

    let upper = relative.to_ascii_uppercase();
    if upper.starts_with(LEGACY_PREFIX) {
        return true;
    }
    SIGNATURE_EXTS.iter().any(|ext| upper.ends_with(ext))
}

/// Удаляет подписи, возвращая оставшиеся записи и число удалённых.
///
/// Порядок остальных записей не меняется.
pub fn strip_signatures(entries: Vec<Entry>) -> (Vec<Entry>, usize) {
    let before = entries.len();
    let mut kept = entries;
    kept.retain(|entry| !is_signature_entry(&entry.name));
    let removed = before - kept.len();
    (kept, removed)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use super::*;

    #[test]
    fn signature_files_under_meta_inf_are_detected() {
        for name in [
            "META-INF/FOO.SF",
            "META-INF/foo.rsa",
            "META-INF/FOO.DSA",
            "META-INF/FOO.EC",
            "META-INF/SIG-CUSTOM",
        ] {
            assert!(is_signature_entry(name), "{name} should be a signature");
        }
    }

    #[test]
    fn manifest_and_resources_are_kept() {
        for name in [
            "META-INF/MANIFEST.MF",
            "META-INF/services/demo.Loader",
            "META-INF/certs/root.SF",
            "demo/App.class",
            "NOT-META-INF/x.SF",
        ] {
            assert!(!is_signature_entry(name), "{name} must be kept");
        }
    }

    #[test]
    fn strip_removes_only_signatures_and_counts_them() {
        let entries = vec![
            Entry::new("META-INF/MANIFEST.MF", b"m".to_vec()),
            Entry::new("META-INF/C.SF", b"s".to_vec()),
            Entry::new("demo/A.class", b"c".to_vec()),
            Entry::new("META-INF/C.RSA", b"b".to_vec()),
        ];

        let (kept, removed) = strip_signatures(entries);
        assert_eq!(removed, 2);
        assert_eq!(
            kept.iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            vec!["META-INF/MANIFEST.MF", "demo/A.class"]
        );
    }
}
