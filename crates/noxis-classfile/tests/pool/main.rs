//! Инвариант писателя: записанный пул не содержит дублей `CONSTANT_Utf8`.
//!
//! Backend не дедуплицирует (`add_*` всегда толкает новый элемент), а HotSpot
//! сопоставляет запись `LocalVariableTypeTable` с `LocalVariableTable` по
//! сырому `name_index`. Дублирующий `Utf8` для одного имени даёт разные индексы
//! и рушит класс: `ClassFormatError: LVTT entry ... does not match any LVT
//! entry`. Писатель обязан интернировать строки — поэтому в записанном пуле
//! каждый `CONSTANT_Utf8` уникален.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::HashSet;
use std::path::PathBuf;

use noxis_classfile::format;
use ristretto_classfile::{ClassFile as RtClass, Constant};

#[test]
fn written_pool_has_no_duplicate_utf8() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    let mut names: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("tests/data")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "class"))
        .collect();
    names.sort();
    assert!(!names.is_empty(), "нет фикстур в tests/data");

    for path in &names {
        let parsed = format::parse(&std::fs::read(path).expect("read")).expect("parse");
        let written = format::write(&parsed).expect("write");
        let raw = RtClass::from_bytes(&written).expect("written parses");
        raw.verify().expect("written verifies");
        let mut seen = HashSet::new();
        for index in 1..=raw.constant_pool.len() {
            if let Some(Constant::Utf8(value)) = raw.constant_pool.get(index as u16) {
                let bytes = value.as_bytes().to_vec();
                assert!(
                    seen.insert(bytes),
                    "дубль CONSTANT_Utf8 {:?} в {}",
                    String::from_utf8_lossy(value.as_bytes()),
                    parsed.this_class
                );
            }
        }
    }
}
