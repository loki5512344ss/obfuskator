//! Type states and per-instruction transfer functions.
//!
//! The stack grows right (`push` appends). `long`/`double` take two slots:
//! value + `Top`. Only the SHAPE is checked (depth, categories), not
//! type agreement - wrong types honestly fail on the JVM verifier.

pub(super) mod calls;
pub(super) mod stack;

use super::merge::descs::{array_element, field_slots, method_slots};
use super::{VerificationType, invalid};
use crate::error::ClassfileError;
use crate::insn::{Arith, ArrayKind, BranchCond, Insn, SlotKind};

use calls::{const_slots, context, convert_shapes, count_slots, invoke, prim_push, prim_slots};
use stack::{dup, dup2, local_get, pop_slots, push, store_local, wide_push};

/// Local types plus the operand stack at one program point.
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct State {
    pub locals: Vec<VerificationType>,
    pub stack: Vec<VerificationType>,
}

/// Apply one instruction to the state (in place).
pub(super) fn transfer(
    class: &str,
    method: &str,
    insn: &Insn,
    offset: u16,
    state: &mut State,
) -> Result<(), ClassfileError> {
    match insn {
        Insn::Mark(label) => Err(invalid(
            class,
            method,
            &format!("marker {label} was not stripped"),
        )),
        Insn::Nop => Ok(()),
        Insn::AconstNull => push(state, vec![VerificationType::Null]),
        Insn::Const(value) => {
            let slots = const_slots(value).map_err(|e| context(class, method, e))?;
            push(state, slots)
        }
        Insn::Load { slot, kind } => {
            let value = local_get(state, *slot, class, method)?;
            let _ = kind;
            push(state, wide_push(value))
        }
        Insn::Store { slot, kind } => {
            let slots = usize::from(*kind == SlotKind::Long || *kind == SlotKind::Double) + 1;
            let taken = pop_slots(state, slots, class, method, "store")?;
            store_local(state, *slot, *kind, taken, class, method)
        }
        Insn::Iinc { index, .. } => {
            let current = local_get(state, *index, class, method)?;
            if current != VerificationType::Integer {
                return Err(invalid(class, method, "iinc over a non-int"));
            }
            Ok(())
        }
        Insn::ArrayLoad(kind) => {
            pop_slots(state, 1, class, method, "array index")?;
            let array = pop_slots(state, 1, class, method, "array")?.remove(0);
            push(state, wide_push(array_element(&array, *kind)))
        }
        Insn::ArrayStore(kind) => {
            let count = match kind {
                ArrayKind::Long | ArrayKind::Double => 2,
                _ => 1,
            };
            pop_slots(state, count, class, method, "value")?;
            pop_slots(state, 1, class, method, "array index")?;
            pop_slots(state, 1, class, method, "array")?;
            Ok(())
        }
        Insn::Pop => {
            let top = pop_slots(state, 1, class, method, "pop")?.remove(0);
            if top == VerificationType::Top {
                return Err(invalid(class, method, "pop of a long/double half"));
            }
            Ok(())
        }
        Insn::Pop2 => {
            pop_slots(state, 2, class, method, "pop2")?;
            Ok(())
        }
        Insn::Dup => dup(state, class, method, &[1], &[1, 1]),
        Insn::DupX1 => dup(state, class, method, &[2, 1], &[1, 2, 1]),
        Insn::DupX2 => dup(state, class, method, &[3, 2, 1], &[1, 3, 2, 1]),
        Insn::Dup2 => dup2(state, class, method, false, false),
        Insn::Dup2X1 => dup2(state, class, method, true, false),
        Insn::Dup2X2 => dup2(state, class, method, true, true),
        Insn::Swap => {
            let mut taken = pop_slots(state, 2, class, method, "swap")?;
            if taken.contains(&VerificationType::Top) {
                return Err(invalid(class, method, "swap with a half"));
            }
            taken.reverse();
            push(state, taken)
        }
        Insn::Binary { prim, op } => {
            let slots = prim_slots(*prim);
            if *op == Arith::Neg {
                pop_slots(state, slots as usize, class, method, "neg")?;
            } else if matches!(op, Arith::Shl | Arith::Shr | Arith::Ushr) {
                // A shift takes (value, int), not two values.
                pop_slots(state, slots as usize + 1, class, method, "shift")?;
            } else {
                pop_slots(state, 2 * slots as usize, class, method, "arith")?;
            }
            push(state, prim_push(*prim))
        }
        Insn::Cmp { prim, .. } => {
            pop_slots(state, 2 * prim_slots(*prim) as usize, class, method, "cmp")?;
            push(state, vec![VerificationType::Integer])
        }
        Insn::Convert(op) => {
            let (src, dst) = convert_shapes(*op);
            pop_slots(state, src, class, method, "convert")?;
            push(state, prim_push(dst))
        }
        Insn::Branch(cond, _) => {
            let count = match cond {
                BranchCond::Zero(..) | BranchCond::Null { .. } => 1,
                BranchCond::Int(..) | BranchCond::Ref { .. } => 2,
            };
            pop_slots(state, count, class, method, "branch")?;
            Ok(())
        }
        Insn::Goto(_) => Ok(()),
        Insn::Jsr(_) | Insn::Ret(_) => Err(invalid(
            class,
            method,
            "jsr/ret subroutines are not supported",
        )),
        Insn::TableSwitch { .. } | Insn::LookupSwitch { .. } => {
            pop_slots(state, 1, class, method, "switch")?;
            Ok(())
        }
        Insn::Return(prim) => {
            if let Some(prim) = prim {
                pop_slots(state, prim_slots(*prim) as usize, class, method, "return")?;
            }
            Ok(())
        }
        Insn::AReturn => {
            pop_slots(state, 1, class, method, "areturn")?;
            Ok(())
        }
        Insn::GetStatic { descriptor, .. } => {
            let slots = field_slots(descriptor).map_err(|e| context(class, method, e))?;
            push(state, slots)
        }
        Insn::PutStatic { descriptor, .. } => {
            let slots = field_slots(descriptor).map_err(|e| context(class, method, e))?;
            pop_slots(state, slots.len(), class, method, "putstatic")?;
            Ok(())
        }
        Insn::GetField { descriptor, .. } => {
            pop_slots(state, 1, class, method, "objectref")?;
            let slots = field_slots(descriptor).map_err(|e| context(class, method, e))?;
            push(state, slots)
        }
        Insn::PutField { descriptor, .. } => {
            let slots = field_slots(descriptor).map_err(|e| context(class, method, e))?;
            pop_slots(state, slots.len(), class, method, "putfield")?;
            pop_slots(state, 1, class, method, "objectref")?;
            Ok(())
        }
        Insn::Invoke {
            kind,
            owner,
            name,
            descriptor,
            ..
        } => invoke(class, method, state, *kind, owner, name, descriptor),
        Insn::InvokeDynamic { descriptor, .. } => {
            let (params, ret) = method_slots(descriptor).map_err(|e| context(class, method, e))?;
            pop_slots(state, count_slots(&params), class, method, "invokedynamic")?;
            if let Some(ret) = ret {
                push(state, ret)?;
            }
            Ok(())
        }
        Insn::New(_) => push(state, vec![VerificationType::Uninitialized(offset)]),
        Insn::NewArray(kind) => {
            pop_slots(state, 1, class, method, "newarray")?;
            let descriptor = match kind {
                ArrayKind::Boolean => "[Z",
                ArrayKind::Char => "[C",
                ArrayKind::Float => "[F",
                ArrayKind::Double => "[D",
                ArrayKind::Byte => "[B",
                ArrayKind::Short => "[S",
                ArrayKind::Int => "[I",
                ArrayKind::Long => "[J",
                ArrayKind::Reference => {
                    return Err(invalid(class, method, "newarray of a reference type"));
                }
            };
            push(
                state,
                vec![VerificationType::Object(descriptor.to_string())],
            )
        }
        Insn::ANewArray(name) => {
            pop_slots(state, 1, class, method, "anewarray")?;
            // `anewarray` adds one dimension: a plain class becomes
            // `[L...;`, an already-array operand (`anewarray [B` for a
            // partially bound multi-dim) gains one more `[`. Wrapping an
            // array operand in `L...;` (`[L[B;`) is not a class.
            let descriptor = if name.starts_with('[') {
                format!("[{name}")
            } else {
                format!("[L{name};")
            };
            push(state, vec![VerificationType::Object(descriptor)])
        }
        Insn::MultiANewArray { descriptor, dims } => {
            pop_slots(state, *dims as usize, class, method, "multianewarray")?;
            push(state, vec![VerificationType::Object(descriptor.clone())])
        }
        Insn::ArrayLength => {
            pop_slots(state, 1, class, method, "arraylength")?;
            push(state, vec![VerificationType::Integer])
        }
        Insn::AThrow => {
            pop_slots(state, 1, class, method, "athrow")?;
            state.stack.clear();
            Ok(())
        }
        Insn::CheckCast(name) => {
            pop_slots(state, 1, class, method, "checkcast")?;
            push(state, vec![VerificationType::Object(name.clone())])
        }
        Insn::InstanceOf(_) => {
            pop_slots(state, 1, class, method, "instanceof")?;
            push(state, vec![VerificationType::Integer])
        }
        Insn::MonitorEnter | Insn::MonitorExit => {
            pop_slots(state, 1, class, method, "monitor")?;
            Ok(())
        }
    }
}
