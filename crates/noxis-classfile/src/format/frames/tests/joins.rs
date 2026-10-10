//! Join-shape tests: loop heads, wide array loads, nested construction.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::super::{StackMapFrame, VerificationType, compute_frames};
use super::{NoHierarchy, code_method, load, load_ref, store, store_ref};
use crate::insn::{ArrayKind, BranchCond, Cond, Const, Insn, Prim};

/// `anewarray [B` (a partially bound multi-dim array) yields its descriptor
/// verbatim: wrapping it again (`[L[B;`) is not a class and HotSpot rejects
/// the pool entry.
#[test]
fn anewarray_of_array_keeps_descriptor() {
    let method = code_method(
        "make",
        "()Ljava/lang/Object;",
        2,
        1,
        vec![
            Insn::Mark(0),
            Insn::Const(Const::Int(3)),
            Insn::ANewArray("[B".to_string()),
            store_ref(0),
            load_ref(0),
            Insn::Branch(BranchCond::Null { is_null: false }, 1),
            Insn::AconstNull,
            store_ref(0),
            Insn::Goto(2),
            Insn::Mark(1),
            Insn::Mark(2),
            load_ref(0),
            Insn::AReturn,
        ],
        vec![],
    );
    let frames = compute_frames("demo/Make", &method, &NoHierarchy).expect("frames");
    assert_eq!(
        frames,
        vec![StackMapFrame::Append {
            offset_delta: 8,
            locals: vec![VerificationType::Object("[[B".to_string())],
        },]
    );
}

/// A loop head keeps `Top` for a slot written only inside the loop.
///
/// Inflows: entry (never wrote it) and back edge (wrote `Integer`). Like
/// `javac`, the join declares `Top`: a precise type would reject the inflow
/// that never wrote the slot (`Type top is not assignable`).
#[test]
fn loop_head_keeps_top_for_loop_local() {
    let method = code_method(
        "sum",
        "()I",
        2,
        3,
        vec![
            Insn::Mark(0),
            Insn::Const(Const::Int(0)),
            store(0),
            Insn::Const(Const::Int(0)),
            store(1),
            Insn::Mark(1),
            load(1),
            Insn::Const(Const::Int(10)),
            Insn::Branch(BranchCond::Int(Cond::Ge), 2),
            load(1),
            store(2),
            load(2),
            store(0),
            Insn::Iinc { index: 1, delta: 1 },
            Insn::Goto(1),
            Insn::Mark(2),
            load(0),
            Insn::Return(Some(Prim::Int)),
        ],
        vec![],
    );
    let frames = compute_frames("demo/Sum", &method, &NoHierarchy).expect("frames");
    assert_eq!(
        frames,
        vec![
            StackMapFrame::Append {
                offset_delta: 4,
                locals: vec![VerificationType::Integer, VerificationType::Integer],
            },
            StackMapFrame::Same { offset_delta: 8 },
        ]
    );
}

/// A nested construction (`new Outer(new Inner())`) initializes only the
/// inner object: the outer `Uninitialized` survives with its own offset, so
/// a later join still sees it instead of a clobbered `Object(Inner)`.
#[test]
fn nested_construction_keeps_outer_uninitialized() {
    let method = code_method(
        "make",
        "()I",
        4,
        1,
        vec![
            Insn::Mark(0),
            Insn::New("demo/Outer".to_string()),
            Insn::Dup,
            Insn::New("demo/Inner".to_string()),
            Insn::Dup,
            Insn::Invoke {
                kind: crate::insn::InvokeKind::Special,
                owner: "demo/Inner".to_string(),
                name: "<init>".to_string(),
                descriptor: "()V".to_string(),
                itf: false,
            },
            Insn::Invoke {
                kind: crate::insn::InvokeKind::Special,
                owner: "demo/Outer".to_string(),
                name: "<init>".to_string(),
                descriptor: "(Ldemo/Inner;)V".to_string(),
                itf: false,
            },
            store_ref(0),
            load_ref(0),
            Insn::Branch(BranchCond::Null { is_null: false }, 1),
            Insn::Const(Const::Int(0)),
            Insn::Goto(2),
            Insn::Mark(1),
            Insn::Const(Const::Int(1)),
            Insn::Mark(2),
            Insn::Return(Some(Prim::Int)),
        ],
        vec![],
    );
    let frames = compute_frames("demo/Make", &method, &NoHierarchy).expect("frames");
    assert_eq!(
        frames,
        vec![
            StackMapFrame::Append {
                offset_delta: 11,
                locals: vec![VerificationType::Object("demo/Outer".to_string())],
            },
            StackMapFrame::SameLocals {
                offset_delta: 0,
                local: VerificationType::Integer,
            },
        ]
    );
}

/// `laload` pushes a pair (`long` + `Top`): a following `lcmp` needs four
/// entries and must not underflow.
#[test]
fn array_load_wide_pushes_pair() {
    let method = code_method(
        "get",
        "([JI)J",
        4,
        2,
        vec![
            Insn::Mark(0),
            load_ref(0),
            load(1),
            Insn::ArrayLoad(crate::insn::ArrayKind::Long),
            Insn::Const(Const::Long(0)),
            Insn::Cmp {
                prim: Prim::Long,
                nan_great: false,
            },
            Insn::Branch(BranchCond::Zero(Cond::Ne), 1),
            Insn::Const(Const::Long(1)),
            Insn::Goto(2),
            Insn::Mark(1),
            Insn::Const(Const::Long(0)),
            Insn::Mark(2),
            Insn::Return(Some(Prim::Long)),
        ],
        vec![],
    );
    let frames = compute_frames("demo/Wide", &method, &NoHierarchy).expect("frames");
    assert_eq!(
        frames,
        vec![
            StackMapFrame::Same { offset_delta: 8 },
            StackMapFrame::SameLocals {
                offset_delta: 0,
                local: VerificationType::Long,
            },
        ]
    );
}

/// `aaload` on a one-dimensional reference array yields the element type.
///
/// Real-world shape (`V8ValueObject.bind` family): one join inflow carries
/// the loaded element (`Method`), the other the array itself (`Method[]`).
/// The join must declare their LUB (`Object`); declaring the array rejects
/// the element inflow (`Type ... is not assignable to ...`).
#[test]
fn aaload_on_ref_array_yields_element() {
    let method = code_method(
        "pick",
        "([Ljava/lang/reflect/Method;)Ljava/lang/Object;",
        2,
        2,
        vec![
            Insn::Mark(0),
            Insn::Const(Const::Int(0)),
            Insn::Branch(BranchCond::Zero(Cond::Eq), 1),
            load_ref(0),
            Insn::Const(Const::Int(0)),
            Insn::ArrayLoad(ArrayKind::Reference),
            store_ref(1),
            Insn::Goto(2),
            Insn::Mark(1),
            load_ref(0),
            store_ref(1),
            Insn::Mark(2),
            load_ref(1),
            Insn::AReturn,
        ],
        vec![],
    );
    let frames = compute_frames("demo/Pick", &method, &NoHierarchy).expect("frames");
    // The join declares locals[1]; whatever the frame kind, it must be the
    // LUB (`Object`), never the array the other inflow carried.
    let mut saw_join = false;
    for frame in &frames {
        let locals: Vec<VerificationType> = match frame {
            StackMapFrame::SameLocals { local, .. } => vec![local.clone()],
            StackMapFrame::Append { locals, .. } | StackMapFrame::Full { locals, .. } => {
                locals.clone()
            }
            StackMapFrame::Same { .. } | StackMapFrame::Chop { .. } => vec![],
        };
        for local in &locals {
            assert_ne!(
                local,
                &VerificationType::Object("[Ljava/lang/reflect/Method;".to_string())
            );
            if local == &VerificationType::Object("java/lang/Object".to_string()) {
                saw_join = true;
            }
        }
    }
    assert!(saw_join, "expected the join LUB in {frames:?}");
}
