//! Spike: пригоден ли `ristretto_classfile` как backend для чтения/записи.
//!
//! Критерии — `docs/stack.md` §2. Тест существует, чтобы ответы на вопросы
//! spike'а не устаревали: если крейт сломает хотя бы один пункт, сборка упадёт,
//! и решение о backend'е придётся пересмотреть.
//!
//! Что здесь **не** проверяется (и что выносится в задачу P1): конвертация
//! нашего IR в их типы и обратно. Spike отвечает только на вопрос «можно ли
//! на этом построить», а не «сколько на это уйдёт времени».

#![allow(clippy::expect_used, clippy::unwrap_used)]

use ristretto_classfile::attributes::{StackFrame, VerificationType};
use ristretto_classfile::{ClassFile, ConstantPool, JAVA_21};

/// Крейт компилируется на заявленном MSRV и собирает валидный класс.
#[test]
fn builds_verifies_and_serializes() {
    let mut constant_pool = ConstantPool::default();
    let this_class = constant_pool.add_class("demo/Hello").expect("add_class");

    let class_file = ClassFile {
        version: JAVA_21,
        constant_pool,
        this_class,
        ..ClassFile::default()
    };
    class_file.verify().expect("verify");

    let mut bytes = Vec::new();
    class_file.to_bytes(&mut bytes).expect("to_bytes");
    assert!(
        bytes.starts_with(&[0xCA, 0xFE, 0xBA, 0xBE]),
        "нет магии CAFEBABE"
    );
}

/// Round-trip: `to_bytes` → `from_bytes` сохраняет содержимое класса.
#[test]
fn round_trip_preserves_identity() {
    let mut constant_pool = ConstantPool::default();
    let this_class = constant_pool.add_class("demo/Round").expect("add_class");
    let source = ClassFile {
        version: JAVA_21,
        constant_pool,
        this_class,
        ..ClassFile::default()
    };

    let mut bytes = Vec::new();
    source.to_bytes(&mut bytes).expect("to_bytes");
    let restored = ClassFile::from_bytes(&bytes).expect("from_bytes");

    assert_eq!(
        restored.class_name().expect("class_name").to_string(),
        "demo/Round"
    );
    assert_eq!(restored.version, source.version);
    assert_eq!(restored.constant_pool.len(), source.constant_pool.len());
}

/// Кадры `StackMapTable` конструируются и сериализуются публичным API —
/// значит, мы можем **записывать** кадры, а не только читать.
#[test]
fn stack_map_frames_are_constructible() {
    let mut constant_pool = ConstantPool::default();
    let this_class = constant_pool.add_class("demo/Frames").expect("add_class");
    let mut class_file = ClassFile {
        version: JAVA_21,
        constant_pool,
        this_class,
        ..ClassFile::default()
    };

    // SameFrame + расширенный кадр с одним элементом на стеке.
    let frames = vec![
        StackFrame::SameFrame { frame_type: 0 },
        StackFrame::SameLocals1StackItemFrameExtended {
            frame_type: 247,
            offset_delta: 3,
            stack: vec![VerificationType::Integer],
        },
    ];

    let name_index = class_file
        .constant_pool
        .add_utf8("StackMapTable")
        .expect("add_utf8");
    class_file
        .attributes
        .push(ristretto_classfile::attributes::Attribute::StackMapTable { name_index, frames });

    let mut bytes = Vec::new();
    class_file.to_bytes(&mut bytes).expect("to_bytes");
    let restored = ClassFile::from_bytes(&bytes).expect("from_bytes");

    let written = restored
        .attributes
        .iter()
        .find_map(|attribute| match attribute {
            ristretto_classfile::attributes::Attribute::StackMapTable { frames, .. } => {
                Some(frames.clone())
            }
            _ => None,
        })
        .expect("атрибут StackMapTable должен пережить round-trip");

    assert_eq!(written.len(), 2);
    assert_eq!(written[0], StackFrame::SameFrame { frame_type: 0 });
}
