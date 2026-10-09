//! Flow acceptance: scheme rotation plus a real-JVM execution check.
//!
//! rotation: 30 methods through the full scheme set must not collapse onto
//! one shape. execution: a branchy `calc` still prints 45 after the pass
//! under `java -Xverify:all` (graceful skip without `java`).

#![allow(clippy::expect_used, clippy::unwrap_used)]

use noxis_classfile::{
    Arith, BranchCond, ClassBuilder, Code, Cond, Const, Insn, InvokeKind, Prim, Program, SlotKind,
    Version, format,
};
use noxis_core::{Config, Context, FlowConfig, Pass};
use noxis_passes::FlowPass;

const ALL_SCHEMES: [&str; 4] = [
    "state-spread",
    "computed-switch",
    "butterfly",
    "exception-goto",
];

/// A small branchy method: `acc=0; for(i=0;i<10;i++) acc+=i; return acc`.
fn branchy_code() -> Code {
    let (head, end) = (100, 101);
    Code {
        max_stack: 2,
        max_locals: 2,
        insns: vec![
            Insn::Const(Const::Int(0)),
            Insn::Store {
                slot: 0,
                kind: SlotKind::Int,
            },
            Insn::Const(Const::Int(0)),
            Insn::Store {
                slot: 1,
                kind: SlotKind::Int,
            },
            Insn::Mark(head),
            Insn::Load {
                slot: 1,
                kind: SlotKind::Int,
            },
            Insn::Const(Const::Int(10)),
            Insn::Branch(BranchCond::Int(Cond::Ge), end),
            Insn::Load {
                slot: 0,
                kind: SlotKind::Int,
            },
            Insn::Load {
                slot: 1,
                kind: SlotKind::Int,
            },
            Insn::Binary {
                prim: Prim::Int,
                op: Arith::Add,
            },
            Insn::Store {
                slot: 0,
                kind: SlotKind::Int,
            },
            Insn::Iinc { index: 1, delta: 1 },
            Insn::Goto(head),
            Insn::Mark(end),
            Insn::Load {
                slot: 0,
                kind: SlotKind::Int,
            },
            Insn::Return(Some(Prim::Int)),
        ],
        exception_table: Vec::new(),
        frames: Vec::new(),
        attributes: Vec::new(),
    }
}

fn program_with(methods: Vec<(String, String, Code)>) -> Program {
    let mut builder = ClassBuilder::new("demo/Flow", Some("java/lang/Object"));
    for (name, descriptor, code) in methods {
        builder = builder.method_with_code(0x0009, name, descriptor, code);
    }
    Program {
        classes: vec![builder.version(Version::JAVA_21).build()],
        resources: Vec::new(),
    }
}

fn run(program: &mut Program, schemes: &[&str]) {
    let config = Config {
        seed: Some(42),
        flow: FlowConfig {
            schemes: schemes.iter().map(|s| s.to_string()).collect(),
            max_insns: 4000,
            encrypt_block_bodies: false,
        },
        ..Config::default()
    };
    let mut ctx = Context::new(config);
    FlowPass.apply(&mut ctx, program).expect("flow pass");
}

fn code_of(program: &Program, name: &str) -> Code {
    program.classes[0]
        .method(name, "()I")
        .expect("method")
        .code
        .clone()
        .expect("code")
}

#[test]
fn rotation_spreads_schemes_across_methods() {
    let methods: Vec<(String, String, Code)> = (0..30)
        .map(|i| (format!("m{i}"), "()I".to_string(), branchy_code()))
        .collect();
    let mut program = program_with(methods);
    run(&mut program, &ALL_SCHEMES);
    let mut shapes = [false; 4];
    for method in &program.classes[0].methods {
        let code = method.code.as_ref().expect("code");
        if code
            .insns
            .iter()
            .any(|i| matches!(i, Insn::TableSwitch { .. }))
        {
            shapes[0] = true;
        }
        if code
            .insns
            .iter()
            .any(|i| matches!(i, Insn::LookupSwitch { .. }))
        {
            shapes[1] = true;
        }
        if !code.exception_table.is_empty() {
            shapes[3] = true;
        }
    }
    let distinct = shapes.iter().filter(|s| **s).count();
    assert!(distinct >= 2, "rotation did not spread: {shapes:?}");
}

/// Every split splice must be verifier-safe: the dead block is entered
/// through an opaque branch (so it owns a frame) and ends with `athrow`
/// (so it joins nothing). A dead `nop` with no incoming edge and no frame
/// is rejected by HotSpot (`Expecting a stack map frame`).
#[test]
fn splits_are_opaque_entered_and_throw() {
    let mut program = program_with(vec![(
        "calc".to_string(),
        "()I".to_string(),
        branchy_code(),
    )]);
    run(&mut program, &["butterfly"]);
    let code = code_of(&program, "calc");
    let insns = &code.insns;
    let throws = insns.iter().filter(|i| matches!(i, Insn::AThrow)).count();
    assert!(throws >= 1, "splits must end dead blocks with athrow");
    for (index, insn) in insns.iter().enumerate() {
        if !matches!(insn, Insn::AThrow) {
            continue;
        }
        let head = &insns[..index];
        assert!(
            matches!(head.last(), Some(Insn::AconstNull)),
            "athrow without a null on top"
        );
        let dead = head
            .iter()
            .rev()
            .find_map(|i| match i {
                Insn::Mark(label) => Some(*label),
                _ => None,
            })
            .expect("dead block without a mark");
        assert!(
            insns
                .iter()
                .any(|i| matches!(i, Insn::Branch(_, target) if *target == dead)),
            "dead mark {dead} has no opaque entry branch"
        );
    }
}

/// `calc` returns the sum 0+..+9 = 45; `main` prints it. After the pass the
/// class must still verify *and* print 45 under a real JVM.
#[test]
fn flow_preserves_semantics_under_java() {
    if std::process::Command::new("java")
        .arg("-version")
        .output()
        .is_err()
    {
        eprintln!("SKIP: no java");
        return;
    }
    let main = Code {
        max_stack: 2,
        max_locals: 1,
        insns: vec![
            Insn::GetStatic {
                owner: "java/lang/System".to_string(),
                name: "out".to_string(),
                descriptor: "Ljava/io/PrintStream;".to_string(),
            },
            Insn::Invoke {
                kind: InvokeKind::Static,
                owner: "demo/Flow".to_string(),
                name: "calc".to_string(),
                descriptor: "()I".to_string(),
                itf: false,
            },
            Insn::Invoke {
                kind: InvokeKind::Virtual,
                owner: "java/io/PrintStream".to_string(),
                name: "println".to_string(),
                descriptor: "(I)V".to_string(),
                itf: false,
            },
            Insn::Return(None),
        ],
        exception_table: Vec::new(),
        frames: Vec::new(),
        attributes: Vec::new(),
    };
    let mut program = program_with(vec![
        ("calc".to_string(), "()I".to_string(), branchy_code()),
        (
            "main".to_string(),
            "([Ljava/lang/String;)V".to_string(),
            main,
        ),
    ]);
    run(&mut program, &ALL_SCHEMES);
    let dir = std::env::temp_dir().join(format!("noxis-flow-{}", std::process::id()));
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
    let output = std::process::Command::new("java")
        .arg("-Xverify:all")
        .arg("-cp")
        .arg(&dir)
        .arg("demo.Flow")
        .output()
        .expect("run java");
    let _ = std::fs::remove_dir_all(&dir);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "java failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(stdout.trim(), "45", "semantics changed: {stdout:?}");
}
