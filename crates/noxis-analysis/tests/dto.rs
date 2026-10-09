//! Databinding keep: `X.class` passed to Gson/Jackson locks the fields of `X`.
//!
//! JSON libraries bind fields by reflection from a `Class` literal, with no
//! field-name string in the bytecode for the existing string-based detection
//! to catch. Renaming such a field silently breaks deserialization, so every
//! field of the literal's class stays.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use noxis_analysis::Analysis;
use noxis_classfile::{ClassBuilder, Code, Const, Field, Insn, InvokeKind, Program, Version};
use noxis_core::Config;

/// `gson.fromJson(reader, State.class)`: `State` has a private field.
fn sample() -> Program {
    let caller = Code {
        max_stack: 3,
        max_locals: 2,
        insns: vec![
            Insn::Load {
                slot: 1,
                kind: noxis_classfile::SlotKind::Ref,
            },
            Insn::Const(Const::Class("demo/State".to_string())),
            Insn::Invoke {
                kind: InvokeKind::Virtual,
                owner: "com/google/gson/Gson".to_string(),
                name: "fromJson".to_string(),
                descriptor: "(Ljava/io/Reader;Ljava/lang/Class;)Ljava/lang/Object;".to_string(),
                itf: false,
            },
            Insn::Pop,
            Insn::Return(None),
        ],
        exception_table: Vec::new(),
        frames: Vec::new(),
        attributes: Vec::new(),
    };
    let mut state = ClassBuilder::new("demo/State", Some("java/lang/Object"))
        .version(Version::JAVA_21)
        .build();
    state.fields.push(Field {
        access_flags: 0x0002,
        name: "nickname".to_string(),
        descriptor: "Ljava/lang/String;".to_string(),
        attributes: Vec::new(),
    });
    let app = ClassBuilder::new("demo/App", Some("java/lang/Object"))
        .version(Version::JAVA_21)
        .method_with_code(0x0009, "load", "()V", caller)
        .build();
    Program {
        classes: vec![state, app],
        resources: Vec::new(),
    }
}

#[test]
fn databind_class_literal_locks_fields() {
    let program = sample();
    let analysis = Analysis::run(&program, &Config::default()).expect("analysis");
    assert!(
        analysis.keep.keeps_member("nickname"),
        "DTO field must survive renaming"
    );
    assert!(
        !analysis.keep.keeps_member("load"),
        "unrelated members must still rename"
    );
}
