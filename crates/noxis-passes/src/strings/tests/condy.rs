//! Condy-mode tests: `CONSTANT_Dynamic` emission, version gate, attribute
//! merging.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use noxis_classfile::{Attribute, BootstrapMethod, Const, Insn, Version};
use noxis_core::{Config, Context, Pass, StringsConfig};
use noxis_runtime::{Scheme, encrypt};

use super::super::StringsPass;
use super::sample;

/// Condensed mode: strings become `CONSTANT_Dynamic` resolved by the
/// decoder's bootstrap method - a concrete-execution deobfuscator sees no
/// decrypt call to follow (todo.md, 2026-10-14: 0 condy changes from SSVM).
#[test]
fn condy_mode_emits_dynamic_constants() {
    let mut program = sample();
    let config = Config {
        seed: Some(7),
        strings: StringsConfig {
            condy: true,
            ..StringsConfig::default()
        },
        ..Config::default()
    };
    let mut ctx = Context::new(config);
    let report = StringsPass.apply(&mut ctx, &mut program).expect("strings");

    assert_eq!(report.transformed, 2);
    let class = &program.classes[0];
    let code = class.methods[0].code.as_ref().expect("code");
    // Both encrypted strings are condy constants, no direct decoder calls.
    let dynamics: Vec<&Const> = code
        .insns
        .iter()
        .filter_map(|insn| match insn {
            Insn::Const(c @ Const::Dynamic { .. }) => Some(c),
            _ => None,
        })
        .collect();
    assert_eq!(dynamics.len(), 3, "1 chunk + 2 chunks: {code:?}");
    for value in &dynamics {
        let Const::Dynamic { descriptor, .. } = value else {
            unreachable!()
        };
        assert_eq!(descriptor, "Ljava/lang/String;");
    }
    assert!(
        !code
            .insns
            .iter()
            .any(|insn| matches!(insn, Insn::Invoke { name, .. } if name == "d")),
        "condy mode must not emit direct decoder calls: {code:?}"
    );
    // The bootstrap entries live on the class and point at the decoder's `b`.
    let decoder = program
        .classes
        .iter()
        .find(|c| c.methods.iter().any(|m| m.name == "b"))
        .expect("decoder with bootstrap method");
    let Attribute::BootstrapMethods(bsm) = class
        .attributes
        .iter()
        .find(|a| matches!(a, Attribute::BootstrapMethods(_)))
        .expect("BootstrapMethods attribute")
    else {
        unreachable!()
    };
    assert_eq!(bsm.len(), 3, "one entry per chunk");
    for entry in bsm {
        assert_eq!(entry.kind, 6, "REF_invokeStatic");
        assert_eq!(entry.owner, decoder.this_class);
        assert_eq!(entry.name, "b");
        assert!(
            matches!(entry.args.as_slice(), [Const::String(_), Const::Int(_)]),
            "args are cipher + salt: {:?}",
            entry.args
        );
    }
    // Bootstrap indices in the constants match the entry positions.
    let indices: Vec<u16> = dynamics
        .iter()
        .map(|c| {
            let Const::Dynamic { bootstrap, .. } = c else {
                unreachable!()
            };
            *bootstrap
        })
        .collect();
    assert_eq!(indices, vec![0, 1, 2]);
    // Ciphertext is still the direct encryption of the originals.
    let schemes = [Scheme::XorSplitmix, Scheme::XteaCtr];
    let originals = ["hello world".to_string(), "y".repeat(50), "y".repeat(50)];
    let ciphers: Vec<String> = bsm
        .iter()
        .map(|e| match e.args[0] {
            Const::String(ref s) => s.clone(),
            _ => unreachable!(),
        })
        .collect();
    let salts: Vec<i32> = bsm
        .iter()
        .map(|e| match e.args[1] {
            Const::Int(s) => s,
            _ => unreachable!(),
        })
        .collect();
    for ((cipher, salt), original) in ciphers.iter().zip(&salts).zip(originals) {
        let scheme = schemes[*salt as usize % schemes.len()];
        assert_eq!(cipher, &encrypt(scheme, 7, *salt, &original).cipher);
    }
}

/// Condy is a Java 11+ feature (classfile 55): older classes keep the direct
/// decoder calls even when the mode is on.
#[test]
fn condy_falls_back_for_old_class_versions() {
    let mut program = sample();
    program.classes[0].version = Version::JAVA_8;
    let config = Config {
        seed: Some(7),
        strings: StringsConfig {
            condy: true,
            ..StringsConfig::default()
        },
        ..Config::default()
    };
    let mut ctx = Context::new(config);
    StringsPass.apply(&mut ctx, &mut program).expect("strings");

    let code = program.classes[0].methods[0].code.as_ref().expect("code");
    assert!(
        code.insns
            .iter()
            .any(|insn| matches!(insn, Insn::Invoke { name, .. } if name == "d")),
        "pre-11 class must keep direct calls: {code:?}"
    );
    assert!(
        !code
            .insns
            .iter()
            .any(|insn| matches!(insn, Insn::Const(Const::Dynamic { .. }))),
        "no condy constants below classfile 55: {code:?}"
    );
}

/// A class that already has `BootstrapMethods` (Kotlin lambdas) keeps its
/// entries: new condy ones are appended and referenced by the right index.
#[test]
fn condy_appends_to_existing_bootstrap_methods() {
    let mut program = sample();
    let existing = BootstrapMethod {
        kind: 6,
        owner: "java/lang/invoke/StringConcatFactory".to_string(),
        name: "makeConcatWithConstants".to_string(),
        descriptor: "([Ljava/lang/Object;)Ljava/lang/String;".to_string(),
        itf: false,
        args: vec![Const::String("\u{1}".to_string())],
    };
    program.classes[0]
        .attributes
        .push(Attribute::BootstrapMethods(vec![existing]));
    let config = Config {
        seed: Some(7),
        strings: StringsConfig {
            condy: true,
            ..StringsConfig::default()
        },
        ..Config::default()
    };
    let mut ctx = Context::new(config);
    StringsPass.apply(&mut ctx, &mut program).expect("strings");

    let class = &program.classes[0];
    let Attribute::BootstrapMethods(bsm) = class
        .attributes
        .iter()
        .find(|a| matches!(a, Attribute::BootstrapMethods(_)))
        .expect("BootstrapMethods attribute")
    else {
        unreachable!()
    };
    // 1 lambda entry + 3 chunk entries.
    assert_eq!(bsm.len(), 4);
    assert_eq!(bsm[0].name, "makeConcatWithConstants");
    let code = class.methods[0].code.as_ref().expect("code");
    let indices: Vec<u16> = code
        .insns
        .iter()
        .filter_map(|insn| match insn {
            Insn::Const(Const::Dynamic { bootstrap, .. }) => Some(*bootstrap),
            _ => None,
        })
        .collect();
    assert_eq!(
        indices,
        vec![1, 2, 3],
        "condy entries start after the lambda"
    );
}
