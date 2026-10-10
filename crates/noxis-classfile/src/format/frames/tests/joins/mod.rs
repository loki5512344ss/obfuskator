//! Join-shape tests: loop heads, wide array loads, nested construction.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::super::{StackMapFrame, VerificationType, compute_frames};

use super::{NoHierarchy, code_method, load, load_ref, store, store_ref};
use crate::insn::{BranchCond, Cond, Const, Insn, Prim};

mod arrays;
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
