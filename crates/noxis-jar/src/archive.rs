//! Чтение и запись ZIP-архива JAR.
//!
//! Единственная задача модуля — переместить записи из/в вектор [`Entry`],
//! сохранив порядок и не потеряв дубликатов молча. Всё, что связано с
//! содержимым (`Program`, маппинги, подпись), живёт в соседних модулях.

use std::fs::File;
use std::io::{Read, Seek, Write};
use std::path::Path;

use thiserror::Error;
use zip::CompressionMethod;
use zip::write::SimpleFileOptions;

/// Ошибки работы с архивом.
#[derive(Debug, Error)]
pub enum JarError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("zip: {0}")]
    Zip(#[from] zip::result::ZipError),

    #[error("duplicate archive entry: {0}")]
    Duplicate(String),

    #[error("class file: {0}")]
    Classfile(#[from] noxis_classfile::ClassfileError),
}

/// Одна запись архива: имя + байты.
///
/// Порядок в `[`Entry`]` — это порядок в архиве, он сохраняется при
/// чтении и записи. Метод сжатия не хранится: всё пишется deflate'ом,
/// что детерминированно и одинаково для одинаковых данных.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub data: Vec<u8>,
}

impl Entry {
    /// Создаёт запись по имени и байтам.
    pub fn new(name: impl Into<String>, data: impl Into<Vec<u8>>) -> Self {
        Self {
            name: name.into(),
            data: data.into(),
        }
    }
}

/// Читает все не-каталожные записи из `reader`, сохраняя порядок.
///
/// Повторяющееся имя — ошибка, а не тихое предупреждение: битый архив
/// с двумя `META-INF/MANIFEST.MF` собрать дороже, чем отказать.
pub fn read_entries<R: Read + Seek>(reader: R) -> Result<Vec<Entry>, JarError> {
    let mut archive = zip::ZipArchive::new(reader)?;
    let mut entries = Vec::with_capacity(archive.len());

    for index in 0..archive.len() {
        let mut file = archive.by_index(index)?;
        if file.is_dir() {
            continue;
        }
        let name = file.name().to_string();
        let mut data = Vec::new();
        file.read_to_end(&mut data)?;
        entries.push(Entry { name, data });
    }

    let names: Vec<String> = entries.iter().map(|entry| entry.name.clone()).collect();
    ensure_unique(&names)?;
    Ok(entries)
}

/// Пишет записи в `writer` в переданном порядке.
pub fn write_entries<W: Write + Seek>(writer: W, entries: &[Entry]) -> Result<(), JarError> {
    let names: Vec<String> = entries.iter().map(|entry| entry.name.clone()).collect();
    ensure_unique(&names)?;

    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    let mut out = zip::ZipWriter::new(writer);
    for entry in entries {
        out.start_file(entry.name.as_str(), options)?;
        out.write_all(&entry.data)?;
    }
    out.finish()?;
    Ok(())
}

/// Читает архив с диска.
pub fn read_file(path: &Path) -> Result<Vec<Entry>, JarError> {
    read_entries(File::open(path)?)
}

/// Пишет архив на диск.
pub fn write_file(path: &Path, entries: &[Entry]) -> Result<(), JarError> {
    let file = File::create(path)?;
    write_entries(file, entries)
}

/// Отклоняет повторяющиеся имена записей.
fn ensure_unique(names: &[String]) -> Result<(), JarError> {
    let mut seen: Vec<&str> = Vec::with_capacity(names.len());
    for name in names {
        if seen.contains(&name.as_str()) {
            return Err(JarError::Duplicate(name.clone()));
        }
        seen.push(name);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use std::io::Cursor;

    use super::*;

    fn entries() -> Vec<Entry> {
        vec![
            Entry::new("META-INF/MANIFEST.MF", b"Manifest-Version: 1.0\n".to_vec()),
            Entry::new("demo/A.class", vec![0xCA, 0xFE, 0xBA, 0xBE]),
            Entry::new("config/app.json", br#"{"k":1}"#.to_vec()),
        ]
    }

    #[test]
    fn round_trip_preserves_order_and_bytes() {
        let source = entries();
        let mut buffer = Vec::new();
        write_entries(Cursor::new(&mut buffer), &source).expect("write");

        let restored = read_entries(Cursor::new(&buffer)).expect("read");
        assert_eq!(restored, source);
    }

    #[test]
    fn duplicate_names_are_rejected() {
        let dup = vec![
            Entry::new("a.txt", b"1".to_vec()),
            Entry::new("a.txt", b"2".to_vec()),
        ];
        let mut buffer = Vec::new();
        let err = write_entries(Cursor::new(&mut buffer), &dup).expect_err("dup");
        assert!(matches!(err, JarError::Duplicate(name) if name == "a.txt"));
    }

    #[test]
    fn missing_file_is_an_io_error() {
        let err = read_file(Path::new("/definitely/not/here.jar")).expect_err("open");
        assert!(matches!(err, JarError::Io(_)));
    }
}
