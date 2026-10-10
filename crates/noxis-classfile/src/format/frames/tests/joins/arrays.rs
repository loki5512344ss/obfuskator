//! Join-shape tests: loop heads, wide array loads, nested construction.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::super::super::{StackMapFrame, VerificationType, compute_frames};
use super::super::{NoHierarchy, code_method, load_ref, store_ref};
use crate::insn::{ArrayKind, BranchCond, Cond, Const, Insn};
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

/// `aaload` on a `Null` arrayref yields `Null`, not `Object`.
///
/// The `Null` path throws (`NullPointerException`) - no value flows from it,
/// so `Null` (bottom) is the precise answer. Yielding `Object` poisons the
/// join permanently: `lub(Object, String)` stays `Object`, while the true
/// merge (`lub(Null, String)`) is `String`. Worse, the poison arrives
/// through *transient* worklist states (before upstream slots refine) and
/// can never leave - the transfer is non-monotone in the arrayref.
#[test]
fn aaload_on_null_yields_null() {
    let method = code_method(
        "pick",
        "(Ljava/lang/String;)Ljava/lang/Object;",
        2,
        2,
        vec![
            Insn::Mark(0),
            Insn::AconstNull,
            store_ref(1),
            Insn::Const(Const::Int(0)),
            Insn::Branch(BranchCond::Zero(Cond::Eq), 1),
            load_ref(1),
            Insn::Const(Const::Int(0)),
            Insn::ArrayLoad(ArrayKind::Reference),
            Insn::Goto(2),
            Insn::Mark(1),
            load_ref(0),
            Insn::Mark(2),
            Insn::AReturn,
        ],
        vec![],
    );
    let frames = compute_frames("demo/Pick", &method, &NoHierarchy).expect("frames");
    let mut saw_string = false;
    for frame in &frames {
        let stack: Vec<VerificationType> = match frame {
            StackMapFrame::SameLocals { local, .. } => vec![local.clone()],
            StackMapFrame::Full { stack, .. } => stack.clone(),
            _ => vec![],
        };
        for item in &stack {
            assert_ne!(
                item,
                &VerificationType::Object("java/lang/Object".to_string()),
                "poisoned join in {frames:?}"
            );
            if item == &VerificationType::Object("java/lang/String".to_string()) {
                saw_string = true;
            }
        }
    }
    assert!(saw_string, "expected the String join in {frames:?}");
}
