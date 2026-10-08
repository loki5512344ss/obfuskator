//! Вызовы и константы: дескрипторы, `<init>`, слоты значений.

use super::super::VerificationType;
use super::super::merge::descs::{DescError, field_slots, method_slots};
use super::State;
use crate::error::ClassfileError;
use crate::insn::{Const, Convert, InvokeKind, Prim};

pub(super) fn const_slots(value: &Const) -> Result<Vec<VerificationType>, DescError> {
    Ok(match value {
        Const::Int(_) => vec![VerificationType::Integer],
        Const::Long(_) => vec![VerificationType::Long, VerificationType::Top],
        Const::Float(_) => vec![VerificationType::Float],
        Const::Double(_) => vec![VerificationType::Double, VerificationType::Top],
        Const::String(_) => vec![VerificationType::Object("java/lang/String".to_string())],
        Const::Class(_) => vec![VerificationType::Object("java/lang/Class".to_string())],
        Const::MethodType(_) => vec![VerificationType::Object(
            "java/lang/invoke/MethodType".to_string(),
        )],
        Const::MethodHandle { .. } => vec![VerificationType::Object(
            "java/lang/invoke/MethodHandle".to_string(),
        )],
        Const::Dynamic { descriptor, .. } => field_slots(descriptor)?,
    })
}

pub(super) fn invoke(
    class: &str,
    method: &str,
    state: &mut State,
    kind: InvokeKind,
    owner: &str,
    name: &str,
    descriptor: &str,
) -> Result<(), ClassfileError> {
    let (params, ret) = method_slots(descriptor).map_err(|e| context(class, method, e))?;
    super::stack::pop_slots(
        state,
        count_slots(&params),
        class,
        method,
        "аргументы вызова",
    )?;
    let mut receiver = None;
    if kind != InvokeKind::Static {
        receiver = super::stack::pop_slots(state, 1, class, method, "objectref")?.pop();
    }
    // `<init>` инициализирует получателя во ВСЕХ его копиях.
    if name == "<init>" {
        match receiver {
            Some(VerificationType::UninitializedThis) => {
                replace(
                    state,
                    &VerificationType::UninitializedThis,
                    &initialized(class),
                );
            }
            Some(VerificationType::Uninitialized(_)) => {
                let fresh = VerificationType::Object(owner.to_string());
                replace_uninit(state, &fresh);
            }
            _ => {}
        }
    }
    if let Some(ret) = ret {
        super::stack::push(state, ret)?;
    }
    Ok(())
}

/// Число слотов всех параметров.
pub(super) fn count_slots(params: &[Vec<VerificationType>]) -> usize {
    params.iter().map(Vec::len).sum()
}

/// Заменить все вхождения типа новым (инициализация через `<init>`).
fn replace(state: &mut State, from: &VerificationType, to: &VerificationType) {
    for slot in state.locals.iter_mut().chain(state.stack.iter_mut()) {
        if slot == from {
            *slot = to.clone();
        }
    }
}

/// Любой `Uninitialized` (кроме `this`) → тип владельца (созданный `new`).
fn replace_uninit(state: &mut State, to: &VerificationType) {
    for slot in state.locals.iter_mut().chain(state.stack.iter_mut()) {
        if matches!(slot, VerificationType::Uninitialized(_)) {
            *slot = to.clone();
        }
    }
}

fn initialized(class: &str) -> VerificationType {
    VerificationType::Object(class.to_string())
}

pub(super) fn context(class: &str, method: &str, error: DescError) -> ClassfileError {
    super::super::invalid(class, method, &error.0)
}

pub(super) fn prim_slots(prim: Prim) -> u32 {
    match prim {
        Prim::Int | Prim::Float => 1,
        Prim::Long | Prim::Double => 2,
    }
}

pub(super) fn prim_push(prim: Prim) -> Vec<VerificationType> {
    match VerificationType::of_prim(prim) {
        value @ (VerificationType::Long | VerificationType::Double) => {
            vec![value, VerificationType::Top]
        }
        value => vec![value],
    }
}

/// `(слоты аргумента, примитив результата)` для конверсии.
pub(super) fn convert_shapes(op: Convert) -> (usize, Prim) {
    use Convert::{D2F, D2I, D2L, F2D, F2I, F2L, I2B, I2C, I2D, I2F, I2L, I2S, L2D, L2F, L2I};
    use Prim::{Double, Float, Int, Long};
    match op {
        I2L => (1, Long),
        I2D => (1, Double),
        I2F | I2B | I2C | I2S => (1, if op == I2F { Float } else { Int }),
        L2I | L2F => (2, if op == L2F { Float } else { Int }),
        L2D => (2, Double),
        F2I | F2L => (1, if op == F2L { Long } else { Int }),
        F2D => (1, Double),
        D2I | D2L => (2, if op == D2L { Long } else { Int }),
        D2F => (2, Float),
    }
}
