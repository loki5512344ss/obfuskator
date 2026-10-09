//! Noise pass tests: junk shape, names, weaving, determinism.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use noxis_classfile::{
    ClassBuilder, Insn, InvokeKind, Program, Resource, Version, empty_code, format,
};
use noxis_core::{Config, NoiseConfig};

use super::*;

fn sample() -> Program {
    Program {
        classes: vec![
            ClassBuilder::new("demo/App", Some("java/lang/Object"))
                .version(Version::JAVA_21)
                .method_with_code(0x0009, "run", "()V", empty_code(0))
                .build(),
        ],
        resources: vec![Resource {
            path: "app.json".into(),
            data: br#"{}"#.to_vec(),
        }],
    }
}

fn noisy() -> NoiseConfig {
    NoiseConfig {
        junk_ratio: 2,
        junk_methods_per_class: 2,
        junk_annotations: 0,
        weave_reachable: true,
    }
}

fn run(program: &mut Program, noise: NoiseConfig) -> PassReport {
    let config = Config {
        seed: Some(42),
        noise,
        ..Config::default()
    };
    let mut ctx = Context::new(config);
    NoisePass.apply(&mut ctx, program).expect("noise pass")
}

fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) if first.is_alphabetic() || first == '_' || first == '$' => {}
        _ => return false,
    }
    chars.all(|c| c.is_alphanumeric() || c == '_' || c == '$')
}

#[test]
fn adds_junk_with_bodies_and_weaves_call() {
    let mut program = sample();
    let report = run(&mut program, noisy());

    assert_eq!(program.classes.len(), 3);
    let junks = &program.classes[1..];
    let mut names = Vec::new();
    for junk in junks {
        // No self-describing fingerprints, valid identifiers, unique.
        assert!(
            !junk.this_class.contains("shadow") && !junk.this_class.contains("decoy"),
            "fingerprint survived: {}",
            junk.this_class
        );
        for part in junk.this_class.split('/') {
            assert!(is_identifier(part), "bad segment in {}", junk.this_class);
        }
        names.push(junk.this_class.clone());
        // One accumulator field, fed by the entry.
        assert_eq!(junk.fields.len(), 1);
        assert_eq!(junk.fields[0].descriptor, "I");
        let field = junk.fields[0].name.clone();
        // Helpers have real branching bodies, not bare returns.
        let helpers: Vec<_> = junk
            .methods
            .iter()
            .filter(|m| m.descriptor == HELPER_DESCRIPTOR)
            .collect();
        assert_eq!(helpers.len(), 2);
        for helper in helpers {
            let code = helper.code.as_ref().expect("code");
            assert!(
                code.insns.iter().any(|i| matches!(i, Insn::Branch(..))),
                "helper without branches"
            );
            assert!(
                code.insns.iter().any(|i| matches!(i, Insn::Binary { .. })),
                "helper without arithmetic"
            );
            assert!(!code.frames.is_empty(), "helper frames were not computed");
        }
        // The entry calls the helpers and stores into the field.
        let entry = junk
            .methods
            .iter()
            .find(|m| m.descriptor == ENTRY_DESCRIPTOR)
            .expect("entry");
        let entry_code = entry.code.as_ref().expect("code");
        let calls = entry_code
            .insns
            .iter()
            .filter(|i| matches!(i, Insn::Invoke { .. }))
            .count();
        assert_eq!(calls, 2, "entry must call every helper");
        assert!(
            entry_code
                .insns
                .iter()
                .any(|i| matches!(i, Insn::PutStatic { name, .. } if name == &field)),
            "entry must feed the field"
        );
    }
    assert_ne!(names[0], names[1]);

    // The live method opens with a call into the first junk entry.
    let live = program.classes[0].methods[0].code.as_ref().unwrap();
    match &live.insns[0] {
        Insn::Invoke {
            kind,
            owner,
            descriptor,
            itf,
            ..
        } => {
            assert_eq!(*kind, InvokeKind::Static);
            assert_eq!(owner, &names[0]);
            assert_eq!(descriptor, "()V");
            assert!(!itf);
        }
        other => panic!("expected Invoke, got: {other:?}"),
    }
    assert_eq!(report.transformed, 3);
    assert_eq!(report.skipped, 0);
}

#[test]
fn disabled_ratio_changes_nothing() {
    let mut program = sample();
    let before = program.clone();
    let report = run(&mut program, NoiseConfig::default());

    assert_eq!(program, before);
    assert_eq!(report.transformed, 0);
    assert_eq!(report.skipped, 0);
}

#[test]
fn junk_is_deterministic() {
    let mut first = sample();
    let mut second = sample();
    run(&mut first, noisy());
    run(&mut second, noisy());
    assert_eq!(first, second);
}

/// Junk bodies carry real branches: they must verify under a real JVM.
#[test]
fn junk_verifies_under_java() {
    if std::process::Command::new("java")
        .arg("-version")
        .output()
        .is_err()
    {
        eprintln!("SKIP: no java");
        return;
    }
    let mut program = sample();
    run(
        &mut program,
        NoiseConfig {
            junk_ratio: 1,
            ..noisy()
        },
    );
    let dir = std::env::temp_dir().join(format!("noxis-noise-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for class in &program.classes {
        let bytes = format::write(class).expect("write");
        let mut path = dir.clone();
        for part in class.this_class.split('/') {
            path.push(part);
        }
        path.set_extension("class");
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(&path, bytes).expect("write class");
    }
    for class in &program.classes {
        let dotted = class.this_class.replace('/', ".");
        let output = std::process::Command::new("java")
            .arg("-Xverify:all")
            .arg("-cp")
            .arg(&dir)
            .arg(&dotted)
            .output()
            .expect("run java");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !stderr.contains("VerifyError") && !stderr.contains("ClassFormatError"),
            "{dotted}: {stderr}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}
