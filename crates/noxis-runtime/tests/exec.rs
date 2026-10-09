//! Сквозная проверка декодера: шифруем в Rust, расшифровываем настоящим `java`.
//!
//! Собираются два класса (декодер + вызывающий `main`), пишутся на диск,
//! `java` исполняет - stdout обязан совпасть с исходными строками. Без `java`
//! тест пропускается (graceful skip, как в `noxis-classfile/tests/frames.rs`).

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;

use noxis_classfile::{ClassBuilder, Code, Const, Insn, InvokeKind, format};
use noxis_runtime::{Scheme, decoder_class, encrypt};

const MASTER: u64 = 0x1234_5678_9ABC_DEF0;
const DECODER: &str = "test/Decoder";
const CALLER: &str = "test/Call";

/// Строки разного размера и алфавита, покрывающие обе схемы.
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

/// Класс-вызыватель: `main` печатает расшифровки.
fn caller_class(parts: &[(String, i32)]) -> noxis_classfile::ClassFile {
    let mut insns = Vec::new();
    for (cipher, salt) in parts {
        insns.push(Insn::GetStatic {
            owner: "java/lang/System".to_string(),
            name: "out".to_string(),
            descriptor: "Ljava/io/PrintStream;".to_string(),
        });
        insns.push(Insn::Const(Const::String(cipher.clone())));
        insns.push(Insn::Const(Const::Int(*salt)));
        insns.push(Insn::Invoke {
            kind: InvokeKind::Static,
            owner: DECODER.to_string(),
            name: "d".to_string(),
            descriptor: "(Ljava/lang/String;I)Ljava/lang/String;".to_string(),
            itf: false,
        });
        insns.push(Insn::Invoke {
            kind: InvokeKind::Virtual,
            owner: "java/io/PrintStream".to_string(),
            name: "println".to_string(),
            descriptor: "(Ljava/lang/String;)V".to_string(),
            itf: false,
        });
    }
    insns.push(Insn::Return(None));
    let code = Code {
        max_stack: 4,
        max_locals: 1,
        insns,
        exception_table: Vec::new(),
        frames: Vec::new(),
        attributes: Vec::new(),
    };
    ClassBuilder::new(CALLER, None)
        .method_with_code(0x0009, "main", "([Ljava/lang/String;)V", code)
        .build()
}

#[test]
fn decoder_executes_under_java() {
    if std::process::Command::new("java")
        .arg("-version")
        .output()
        .is_err()
    {
        eprintln!("SKIP: нет java");
        return;
    }
    let samples = samples();
    let mut parts = Vec::new();
    let mut expected = Vec::new();
    for (index, (text, scheme)) in samples.iter().enumerate() {
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
    let caller = caller_class(&parts);

    let dir: PathBuf = std::env::temp_dir().join(format!("noxis-exec-{}", std::process::id()));
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
    assert_eq!(lines, want, "расшифровка не сошлась");
}
