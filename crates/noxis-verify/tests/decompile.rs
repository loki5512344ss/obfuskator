//! Decompile-quality metric: jadx must chew a clean jar without warnings.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use noxis_classfile::{ClassBuilder, Code, Insn, Version};
use noxis_verify::{decompile_report, jadx_available};

/// Two classes, one with a branchy method: enough structure for jadx to
/// prove it can read the jar, small enough to run in milliseconds.
fn sample_jar(path: &std::path::Path) {
    let loop_code = Code {
        max_stack: 2,
        max_locals: 2,
        insns: vec![
            Insn::Const(noxis_classfile::Const::Int(0)),
            Insn::Store {
                slot: 0,
                kind: noxis_classfile::SlotKind::Int,
            },
            Insn::Mark(0),
            Insn::Load {
                slot: 0,
                kind: noxis_classfile::SlotKind::Int,
            },
            Insn::Const(noxis_classfile::Const::Int(10)),
            Insn::Branch(
                noxis_classfile::BranchCond::Int(noxis_classfile::Cond::Ge),
                1,
            ),
            Insn::Load {
                slot: 0,
                kind: noxis_classfile::SlotKind::Int,
            },
            Insn::Const(noxis_classfile::Const::Int(1)),
            Insn::Binary {
                prim: noxis_classfile::Prim::Int,
                op: noxis_classfile::Arith::Add,
            },
            Insn::Store {
                slot: 0,
                kind: noxis_classfile::SlotKind::Int,
            },
            Insn::Goto(0),
            Insn::Mark(1),
            Insn::Load {
                slot: 0,
                kind: noxis_classfile::SlotKind::Int,
            },
            Insn::Return(Some(noxis_classfile::Prim::Int)),
        ],
        exception_table: Vec::new(),
        frames: Vec::new(),
        attributes: Vec::new(),
    };
    let mut program = noxis_classfile::Program {
        classes: vec![
            ClassBuilder::new("demo/App", Some("java/lang/Object"))
                .version(Version::JAVA_21)
                .method_with_code(0x0009, "run", "()I", loop_code)
                .build(),
            ClassBuilder::new("demo/Help", Some("java/lang/Object"))
                .version(Version::JAVA_21)
                .build(),
        ],
        resources: Vec::new(),
    };
    // Real frames: jadx grades structure, not just parsing.
    let hierarchy = noxis_analysis::ClassHierarchy::build(&program);
    for class in &mut program.classes {
        for method in &mut class.methods {
            if method.code.is_none() {
                continue;
            }
            let snapshot = method.clone();
            let frames = noxis_classfile::compute_frames(&class.this_class, &snapshot, &hierarchy)
                .expect("frames");
            if let Some(code) = method.code.as_mut() {
                code.frames = frames;
            }
        }
    }
    let entries = noxis_jar::save(&program).expect("save");
    noxis_jar::write_file(path, &entries).expect("write jar");
}

#[test]
fn clean_jar_decompiles_without_warnings() {
    if !jadx_available() {
        eprintln!("SKIP: no jadx");
        return;
    }
    let dir = std::env::temp_dir().join(format!("noxis-jadx-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let jar = dir.join("in.jar");
    sample_jar(&jar);
    let report = decompile_report(&jar).expect("jadx runs");
    assert!(
        report.files >= 2,
        "expected 2 classes, got {}",
        report.files
    );
    assert_eq!(report.failed, 0, "clean code must not fail: {report:?}");
    assert_eq!(
        report.files_with_warn, 0,
        "clean code must not warn: {report:?}"
    );
}
