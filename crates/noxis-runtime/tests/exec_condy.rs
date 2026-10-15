//! Condy-приёмка: шифруем в Rust, расшифровывает настоящая JVM через
//! bootstrap-метод декодера (`CONSTANT_Dynamic`).
//!
//! Это приёмка приоритета 1 из todo.md (2026-10-14): sandbox-деобфускатор
//! (narumii/SSVM) не резолвит condy вообще - 0 изменений на condy-классе,
//! тогда как прямой вызов декодера он исполняет и снимает строку. Метод `b`
//! живёт в декодере и вызывается самой JVM в момент `ldc`.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;

use noxis_classfile::{Attribute, BootstrapMethod, ClassBuilder, Code, Const, Insn, format};
use noxis_runtime::{BOOTSTRAP_DESC, BOOTSTRAP_METHOD, Scheme, decoder_class, encrypt};

const MASTER: u64 = 0x1234_5678_9ABC_DEF0;
const DECODER: &str = "test/CondDecoder";
const CALLER: &str = "test/CondCall";

/// Строки обоих схем - тот же набор, что у прямого вызова.
fn samples() -> Vec<(String, Scheme)> {
    vec![
        ("hi".to_string(), Scheme::XorSplitmix),
        ("Hello, мир!".to_string(), Scheme::XorSplitmix),
        ("x".repeat(100), Scheme::XorSplitmix),
        ("hi".to_string(), Scheme::XteaCtr),
        ("Hello, мир!".to_string(), Scheme::XteaCtr),
        ("x".repeat(100), Scheme::XteaCtr),
    ]
}

/// Класс-вызыватель condy-режима: `main` печатает `ldc CONSTANT_Dynamic`.
///
/// Ровно то, что эмитит пасс `strings` при `condy = true`: по одной
/// bootstrap-записи на чанк (cipher + salt как статические аргументы),
/// константа со стандартным дескриптором `Ljava/lang/String;`.
fn condy_caller_class(parts: &[(String, i32)]) -> noxis_classfile::ClassFile {
    let mut insns = Vec::new();
    let mut bootstrap: Vec<BootstrapMethod> = Vec::new();
    for (cipher, salt) in parts {
        bootstrap.push(BootstrapMethod {
            kind: 6, // REF_invokeStatic
            owner: DECODER.to_string(),
            name: BOOTSTRAP_METHOD.to_string(),
            descriptor: BOOTSTRAP_DESC.to_string(),
            itf: false,
            args: vec![Const::String(cipher.clone()), Const::Int(*salt)],
        });
        insns.push(Insn::GetStatic {
            owner: "java/lang/System".to_string(),
            name: "out".to_string(),
            descriptor: "Ljava/io/PrintStream;".to_string(),
        });
        insns.push(Insn::Const(Const::Dynamic {
            name: "_".to_string(),
            descriptor: "Ljava/lang/String;".to_string(),
            bootstrap: (bootstrap.len() - 1) as u16,
        }));
        insns.push(Insn::Invoke {
            kind: noxis_classfile::InvokeKind::Virtual,
            owner: "java/io/PrintStream".to_string(),
            name: "println".to_string(),
            descriptor: "(Ljava/lang/String;)V".to_string(),
            itf: false,
        });
    }
    insns.push(Insn::Return(None));
    let code = Code {
        max_stack: 2,
        max_locals: 1,
        insns,
        exception_table: Vec::new(),
        frames: Vec::new(),
        attributes: Vec::new(),
    };
    ClassBuilder::new(CALLER, None)
        .attribute(Attribute::BootstrapMethods(bootstrap))
        .method_with_code(0x0009, "main", "([Ljava/lang/String;)V", code)
        .build()
}

#[test]
fn condy_bootstrap_executes_under_java() {
    if std::process::Command::new("java")
        .arg("-version")
        .output()
        .is_err()
    {
        eprintln!("SKIP: нет java");
        return;
    }
    let mut parts = Vec::new();
    let mut expected = Vec::new();
    for (index, (text, scheme)) in samples().iter().enumerate() {
        let salt = (index as i32)
            .saturating_mul(2)
            .saturating_add(match scheme {
                Scheme::XorSplitmix => 0,
                Scheme::XteaCtr => 1,
            });
        let blob = encrypt(*scheme, MASTER, salt, text);
        parts.push((blob.cipher, salt));
        expected.push(text.to_string());
    }
    let decoder = decoder_class(DECODER, MASTER, &["xor-splitmix", "xtea-ctr"]).expect("decoder");
    let caller = condy_caller_class(&parts);

    let dir: PathBuf = std::env::temp_dir().join(format!("noxis-condy-{}", std::process::id()));
    for (name, class) in [(DECODER, &decoder), (CALLER, &caller)] {
        let mut path = dir.clone();
        for part in name.split('/') {
            path.push(part);
        }
        path.set_extension("class");
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(&path, format::write(class).expect("write")).expect("write class");
    }
    let out = std::process::Command::new("java")
        .arg("-Xverify:all")
        .arg("-cp")
        .arg(&dir)
        .arg(CALLER.replace('/', "."))
        .output()
        .expect("запуск java");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(out.status.success(), "java упал:\n{stderr}\n{stdout}");
    let lines: Vec<&str> = stdout.lines().collect();
    let want: Vec<&str> = expected.iter().map(String::as_str).collect();
    assert_eq!(lines, want, "condy-расшифровка не сошлась");
}
