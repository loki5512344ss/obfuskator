//! Acceptance for `compute_frames`: fixtures plus emitter edge cases.
//!
//! Frames are dropped and recomputed. Judges: determinism + IR round-trip +
//! real `java -Xverify:all` for classes without external dependencies.
//! (The backend's `verify_class` is unfit for acceptance: it expects frames
//! in bytes while `from_bytes` hands out indices - see `todo.md`.)

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use noxis_classfile::{
    Arith, BranchCond, ClassBuilder, Code, Cond, Const, Insn, Prim, SlotKind, Version,
    {Hierarchy, compute_frames, format},
};

/// A `class → superclass` map for LUB.
#[derive(Default)]
struct MapHierarchy {
    supers: BTreeMap<String, String>,
}

impl Hierarchy for MapHierarchy {
    fn superclass(&self, class: &str) -> Option<String> {
        self.supers.get(class).cloned()
    }
}

/// JDK-only classes - `java` checks them fully.
const JDK_ONLY: [&str; 2] = [
    "dev/loki/lorep/util/TimeUtil",
    "dev/loki/lorep/util/PaginationUtil",
];

/// Fixtures: drop frames, recompute, check the output.
#[test]
fn fixtures_recompute_and_verify() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    let mut names: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("tests/data")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "class"))
        .collect();
    names.sort();
    assert!(!names.is_empty(), "no fixtures in tests/data");

    let mut supers = BTreeMap::new();
    let mut parsed = Vec::new();
    for name in &names {
        let class = format::parse(&std::fs::read(name).expect("read")).expect("parse");
        supers.insert(class.this_class.clone(), class.super_class.clone());
        parsed.push(class);
    }
    // Only `java/lang/Object` has no superclass.
    supers.entry("java/lang/Object".to_string()).or_insert(None);
    let hierarchy = MapHierarchy {
        supers: supers
            .into_iter()
            .filter_map(|(k, v)| v.map(|sup| (k, sup)))
            .collect(),
    };

    let scratch = std::env::temp_dir().join(format!("noxis-frames-{}", std::process::id()));
    let java = java_available();
    for class in &mut parsed {
        if class.version.major < 50 {
            continue;
        }
        for method in &mut class.methods {
            if method.code.is_none() {
                continue;
            }
            let first = compute_frames(&class.this_class, method, &hierarchy).expect("recompute");
            method.code.as_mut().expect("code").frames = first.clone();
            // Determinism: a second run yields the same frames.
            let second = compute_frames(&class.this_class, method, &hierarchy).expect("recompute");
            assert_eq!(first, second, "nondeterministic frames");
        }
        // Round-trip: written bytes read back into the same IR (frames included).
        let written = format::write(class).expect("write");
        assert_eq!(
            format::parse(&written).expect("re-parse"),
            *class,
            "round-trip"
        );
        if java && JDK_ONLY.contains(&class.this_class.as_str()) {
            java_verify(&scratch, class, &written).expect("java -Xverify:all");
        }
    }
    let _ = std::fs::remove_dir_all(&scratch);
}

/// A join with a `double` below on the stack must emit it as ONE entry.
///
/// Regression: the emitter used to write the slot pair (`[Double, Top]`),
/// like `javac` does for locals - but on the operand stack `javac` writes a
/// bare `[Double]`, and HotSpot rejects the pair with
/// `Inconsistent stackmap frames`.
#[test]
fn wide_stack_join_emits_single_entries() {
    // iload_0, i2d, iload_1, <opaque join>, i2d, ddiv, d2i, ireturn.
    // At the join the stack holds a double plus an int.
    let (dead, into) = (100, 101);
    let mut insns = vec![
        Insn::Load {
            slot: 0,
            kind: SlotKind::Int,
        },
        Insn::Convert(noxis_classfile::Convert::I2D),
        Insn::Load {
            slot: 1,
            kind: SlotKind::Int,
        },
    ];
    insns.extend([
        Insn::Const(Const::Int(7)),
        Insn::Dup,
        Insn::Binary {
            prim: Prim::Int,
            op: Arith::Mul,
        },
        Insn::Const(Const::Int(7)),
        Insn::Binary {
            prim: Prim::Int,
            op: Arith::Add,
        },
        Insn::Const(Const::Int(1)),
        Insn::Binary {
            prim: Prim::Int,
            op: Arith::And,
        },
        Insn::Branch(BranchCond::Zero(Cond::Ne), dead),
        Insn::Goto(into),
        Insn::Mark(dead),
        Insn::Nop,
        Insn::Mark(into),
        Insn::Convert(noxis_classfile::Convert::I2D),
        Insn::Binary {
            prim: Prim::Double,
            op: Arith::Div,
        },
        Insn::Convert(noxis_classfile::Convert::D2I),
        Insn::Return(Some(Prim::Int)),
    ]);
    let code = Code {
        max_stack: 6,
        max_locals: 2,
        insns,
        exception_table: Vec::new(),
        frames: Vec::new(),
        attributes: Vec::new(),
    };
    let class = ClassBuilder::new("demo/Wide", Some("java/lang/Object"))
        .version(Version::JAVA_21)
        .method_with_code(0x0009, "calc", "(II)I", code)
        .build();
    let hierarchy = MapHierarchy::default();
    let frames = compute_frames(&class.this_class, &class.methods[0], &hierarchy).expect("frames");
    let mut class = class;
    class.methods[0].code.as_mut().expect("code").frames = frames;
    if !java_available() {
        eprintln!("SKIP: no java");
        return;
    }
    let scratch = std::env::temp_dir().join(format!("noxis-wide-{}", std::process::id()));
    let bytes = format::write(&class).expect("write");
    java_verify(&scratch, &class, &bytes).expect("java -Xverify:all");
    let _ = std::fs::remove_dir_all(&scratch);
}

/// Whether `java` is available for checks with the real verifier.
fn java_available() -> bool {
    Command::new("java")
        .arg("-version")
        .output()
        .is_ok_and(|out| out.status.success())
}

/// Write a class into a package tree and run `java -Xverify:all`.
/// Success means loading without verification errors (`main` may be missing).
fn java_verify(
    scratch: &Path,
    class: &noxis_classfile::ClassFile,
    bytes: &[u8],
) -> Result<(), String> {
    let mut path = scratch.to_path_buf();
    for part in class.this_class.split('/') {
        path.push(part);
    }
    path.set_extension("class");
    std::fs::create_dir_all(path.parent().expect("parent")).map_err(|e| format!("mkdir: {e}"))?;
    std::fs::write(&path, bytes).map_err(|e| format!("write: {e}"))?;
    let dotted = class.this_class.replace('/', ".");
    let out = Command::new("java")
        .arg("-Xverify:all")
        .arg("-cp")
        .arg(scratch)
        .arg(&dotted)
        .output()
        .map_err(|e| format!("running java: {e}"))?;
    let stderr = String::from_utf8_lossy(&out.stderr);
    for bad in [
        "VerifyError",
        "ClassFormatError",
        "NoClassDefFoundError",
        "LinkageError",
    ] {
        if stderr.contains(bad) {
            return Err(format!("{dotted}:\n{stderr}"));
        }
    }
    Ok(())
}
