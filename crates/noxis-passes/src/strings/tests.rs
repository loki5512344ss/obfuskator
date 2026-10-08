//! Тесты пасса строк: подстановка, чанки, детерминизм шифротекста.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use noxis_classfile::{ClassBuilder, Code, Const, Insn, Program, Version};
use noxis_core::{Config, Context};
use noxis_runtime::{Scheme, encrypt};

use super::StringsPass;
use super::crypt::{decode_sequence, salt_for_scheme, split_chunks};
use noxis_core::Pass;

fn sample() -> Program {
    let code = Code {
        max_stack: 1,
        max_locals: 0,
        insns: vec![
            Insn::Const(Const::String("hello world".to_string())),
            Insn::Const(Const::String("x".to_string())),
            Insn::Const(Const::String("y".repeat(100))),
            Insn::Const(Const::Int(5)),
            Insn::Return(None),
        ],
        exception_table: Vec::new(),
        frames: Vec::new(),
        attributes: Vec::new(),
    };
    Program {
        classes: vec![
            ClassBuilder::new("demo/App", Some("java/lang/Object"))
                .version(Version::JAVA_21)
                .method_with_code(0x0009, "run", "()V", code)
                .build(),
        ],
        resources: Vec::new(),
    }
}

/// Короткие строки не режутся, длинные — на границе символов.
#[test]
fn chunks_split_by_chars() {
    assert_eq!(split_chunks("abc", 64), vec!["abc".to_string()]);
    let chunks = split_chunks(&"y".repeat(100), 64);
    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks.concat().len(), 100);
}

/// Salt всегда неотрицателен и попадает в схему.
#[test]
fn salt_hits_scheme() {
    for raw in [0u64, 1, 7, u64::MAX, 123456789] {
        for (index, total) in [(0, 1), (0, 2), (1, 2), (2, 3)] {
            let salt = salt_for_scheme(raw, index, total);
            assert!(salt >= 0, "отрицательный salt");
            assert_eq!(salt as usize % total, index);
        }
    }
}

/// Пасс: строки заменены вызовами, шифротекст воспроизводим, декодер есть.
#[test]
fn strings_replaced_and_deterministic() {
    let mut program = sample();
    let config = Config {
        seed: Some(7),
        ..Config::default()
    };
    let mut ctx = Context::new(config.clone());
    let report = StringsPass.apply(&mut ctx, &mut program).expect("strings");

    assert_eq!(report.transformed, 2);
    assert_eq!(report.skipped, 1);
    // Декодер добавлен последним.
    assert_eq!(program.classes.len(), 2);
    let decoder = program.classes.last().expect("декодер");
    assert!(decoder.methods.iter().any(|m| m.name == "d"));

    // Исходных строк не осталось (шифры — длинные Const::String — на месте);
    // короткая на месте.
    let code = program.classes[0].methods[0].code.as_ref().expect("код");
    let kept: Vec<&String> = code
        .insns
        .iter()
        .filter_map(|insn| match insn {
            Insn::Const(Const::String(s)) => Some(s),
            _ => None,
        })
        .collect();
    assert!(!kept.contains(&&"hello world".to_string()));
    assert!(!kept.iter().any(|s| *s == &"y".repeat(100)));
    assert!(kept.contains(&&"x".to_string()));

    // Каждый вызов сверяется с прямым шифрованием (тот же сид → те же блобы).
    let schemes = [Scheme::XorSplitmix, Scheme::XteaCtr];
    let mut triples = Vec::new();
    let mut window: Vec<&Insn> = Vec::new();
    for insn in &code.insns {
        window.push(insn);
        if window.len() > 3 {
            window.remove(0);
        }
        if let [
            Insn::Const(Const::String(cipher)),
            Insn::Const(Const::Int(salt)),
            Insn::Invoke { name, .. },
        ] = &window[..]
            && name == "d"
        {
            triples.push((cipher.clone(), *salt));
        }
    }
    // "hello world" (1 чанк) + "y"*100 (2 чанка).
    assert_eq!(triples.len(), 3);
    let originals = ["hello world".to_string(), "y".repeat(50), "y".repeat(50)];
    for ((cipher, salt), original) in triples.iter().zip(originals) {
        let scheme = schemes[*salt as usize % schemes.len()];
        let blob = encrypt(scheme, 7, *salt, &original);
        assert_eq!(cipher, &blob.cipher);
    }
    // Склейка чанков через `concat` — ровно одна; конструктор жив.
    assert_eq!(
        code.insns
            .iter()
            .filter(|insn| matches!(insn, Insn::Invoke { name, .. } if name == "concat"))
            .count(),
        1
    );
    assert!(!decode_sequence("o", &[("c".to_string(), 1)]).is_empty());
}
