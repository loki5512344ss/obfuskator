//! Unit tests for `compute_frames` on synthesized IR.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::{StackMapFrame, VerificationType, compute_frames};
use crate::Hierarchy;
use crate::insn::{BranchCond, Cond, Const, Insn, Prim, SlotKind};
use crate::model::{Code, ExceptionHandler, Method};

/// Empty hierarchy: everything unknown descends from `Object` (enough for LUB).
pub(super) struct NoHierarchy;

impl Hierarchy for NoHierarchy {
    fn superclass(&self, _class: &str) -> Option<String> {
        None
    }
}

pub(super) fn code_method(
    name: &str,
    descriptor: &str,
    max_stack: u16,
    max_locals: u16,
    insns: Vec<Insn>,
    table: Vec<ExceptionHandler>,
) -> Method {
    Method {
        access_flags: 0x0008,
        name: name.to_string(),
        descriptor: descriptor.to_string(),
        code: Some(Code {
            max_stack,
            max_locals,
            insns,
            exception_table: table,
            frames: vec![],
            attributes: vec![],
        }),
        attributes: vec![],
    }
}

/// A branch emits two frames (`Same`, `SameLocals`).
#[test]
fn branch_emits_same_frames() {
    let mut method = code_method(
        "max",
        "(II)I",
        2,
        2,
        vec![
            Insn::Mark(0),
            load(0),
            load(1),
            Insn::Branch(BranchCond::Int(Cond::Ge), 1),
            Insn::Const(Const::Int(0)),
            Insn::Goto(2),
            Insn::Mark(1),
            Insn::Const(Const::Int(1)),
            Insn::Mark(2),
            Insn::Return(Some(Prim::Int)),
        ],
        vec![],
    );
    method.access_flags = 0x0009;
    let frames = compute_frames("demo/Max", &method, &NoHierarchy).expect("frames");
    assert_eq!(
        frames,
        vec![
            StackMapFrame::Same { offset_delta: 5 },
            StackMapFrame::SameLocals {
                offset_delta: 0,
                local: VerificationType::Integer,
            },
        ]
    );
}

/// `try/catch`: the handler gets `Full`, the continuation - `Same`.
///
/// Slot 1 is written only inside the handler, so the join keeps `Top` there
/// (like `javac`): a precise type would reject the inflow that never wrote
/// it (`Type top is not assignable`).
#[test]
fn handler_emits_full_frame() {
    let method = code_method(
        "run",
        "()V",
        1,
        2,
        vec![
            Insn::Mark(0),
            Insn::Const(Const::Int(0)),
            store(0),
            Insn::Mark(1),
            Insn::Const(Const::Int(1)),
            store(0),
            Insn::Goto(3),
            Insn::Mark(2),
            store_ref(1),
            Insn::Const(Const::Int(2)),
            store(0),
            Insn::Mark(3),
            Insn::Return(None),
        ],
        vec![ExceptionHandler {
            start: 1,
            end: 2,
            handler: 2,
            catch_type: Some("java/lang/Exception".to_string()),
        }],
    );
    let frames = compute_frames("demo/Try", &method, &NoHierarchy).expect("frames");
    assert_eq!(
        frames,
        vec![
            StackMapFrame::Full {
                offset_delta: 5,
                locals: vec![VerificationType::Integer],
                stack: vec![VerificationType::Object("java/lang/Exception".to_string())],
            },
            StackMapFrame::Same { offset_delta: 2 },
        ]
    );
}

/// Two `new` on different branches cannot merge - an honest error, not `Top`.
#[test]
fn uninitialized_merge_is_rejected() {
    let method = code_method(
        "pick",
        "(Z)Ljava/lang/Object;",
        1,
        2,
        vec![
            Insn::Mark(0),
            load(0),
            Insn::Branch(BranchCond::Zero(Cond::Ne), 1),
            Insn::New("a/A".to_string()),
            store_ref(1),
            Insn::Goto(2),
            Insn::Mark(1),
            Insn::New("a/A".to_string()),
            store_ref(1),
            Insn::Mark(2),
            Insn::Load {
                slot: 1,
                kind: SlotKind::Ref,
            },
            Insn::AReturn,
        ],
        vec![],
    );
    let err = compute_frames("demo/Pick", &method, &NoHierarchy).expect_err("merge");
    assert!(
        err.to_string().contains("uninitialized"),
        "wrong message: {err}"
    );
}

mod joins;

pub(super) fn load(slot: u16) -> Insn {
    Insn::Load {
        slot,
        kind: SlotKind::Int,
    }
}

pub(super) fn load_ref(slot: u16) -> Insn {
    Insn::Load {
        slot,
        kind: SlotKind::Ref,
    }
}

pub(super) fn store(slot: u16) -> Insn {
    Insn::Store {
        slot,
        kind: SlotKind::Int,
    }
}

pub(super) fn store_ref(slot: u16) -> Insn {
    Insn::Store {
        slot,
        kind: SlotKind::Ref,
    }
}
