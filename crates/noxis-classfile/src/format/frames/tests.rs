//! Unit-тесты `compute_frames` на синтезированном IR.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::{StackMapFrame, VerificationType, compute_frames};
use crate::Hierarchy;
use crate::insn::{BranchCond, Cond, Const, Insn, Prim, SlotKind};
use crate::model::{Code, ExceptionHandler, Method};

/// Пустая иерархия: всё неизвестное — наследники `Object` (для LUB хватает).
struct NoHierarchy;

impl Hierarchy for NoHierarchy {
    fn superclass(&self, _class: &str) -> Option<String> {
        None
    }
}

fn code_method(
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

/// Ветвь → два кадра (`Same`, `SameLocals`).
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

/// `try/catch`: handler получает `Full`, продолжение — `Append`.
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
            StackMapFrame::Append {
                offset_delta: 2,
                locals: vec![VerificationType::Object("java/lang/Exception".to_string())],
            },
        ]
    );
}

/// Два `new` на разных ветвях нельзя слить — честная ошибка, не `Top`.
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
    let err = compute_frames("demo/Pick", &method, &NoHierarchy).expect_err("слияние");
    assert!(
        err.to_string().contains("неинициализированных"),
        "не тот текст: {err}"
    );
}

fn load(slot: u16) -> Insn {
    Insn::Load {
        slot,
        kind: SlotKind::Int,
    }
}

fn store(slot: u16) -> Insn {
    Insn::Store {
        slot,
        kind: SlotKind::Int,
    }
}

fn store_ref(slot: u16) -> Insn {
    Insn::Store {
        slot,
        kind: SlotKind::Ref,
    }
}
