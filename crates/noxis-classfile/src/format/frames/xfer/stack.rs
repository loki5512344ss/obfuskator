//! Стек и locals: снятие/укладка слотов, `dup*`.

use super::super::{VerificationType, invalid};
use super::State;
use crate::error::ClassfileError;
use crate::insn::SlotKind;

/// Запись в local с учётом ширины (`long`/`double` — парой с `Top`).
pub(super) fn store_local(
    state: &mut State,
    slot: u16,
    kind: SlotKind,
    taken: Vec<VerificationType>,
    class: &str,
    method: &str,
) -> Result<(), ClassfileError> {
    let wide = kind == SlotKind::Long || kind == SlotKind::Double;
    if taken.len() != usize::from(wide) + 1 {
        return Err(invalid(class, method, "ширина store не совпала"));
    }
    let mut taken = taken;
    local_set(state, slot, taken.remove(0), class, method)?;
    if wide {
        local_set(
            state,
            slot.checked_add(1)
                .ok_or_else(|| invalid(class, method, "слот вне u16"))?,
            VerificationType::Top,
            class,
            method,
        )?;
    }
    Ok(())
}

pub(super) fn push(state: &mut State, slots: Vec<VerificationType>) -> Result<(), ClassfileError> {
    state.stack.extend(slots);
    Ok(())
}

/// Снять `n` слотов (снизу вверх); нехватка — честная ошибка.
pub(super) fn pop_slots(
    state: &mut State,
    n: usize,
    class: &str,
    method: &str,
    what: &str,
) -> Result<Vec<VerificationType>, ClassfileError> {
    if state.stack.len() < n {
        return Err(invalid(
            class,
            method,
            &format!("переполнение стека вниз в {what}"),
        ));
    }
    Ok(state.stack.split_off(state.stack.len() - n))
}

pub(super) fn local_get(
    state: &State,
    slot: u16,
    class: &str,
    method: &str,
) -> Result<VerificationType, ClassfileError> {
    state
        .locals
        .get(slot as usize)
        .cloned()
        .ok_or_else(|| invalid(class, method, &format!("чтение слота {slot} вне locals")))
}

pub(super) fn local_set(
    state: &mut State,
    slot: u16,
    value: VerificationType,
    class: &str,
    method: &str,
) -> Result<(), ClassfileError> {
    let cell = state
        .locals
        .get_mut(slot as usize)
        .ok_or_else(|| invalid(class, method, &format!("запись слота {slot} вне locals")))?;
    *cell = value;
    Ok(())
}

/// Значение local'а целиком: `long`/`double` — парой с `Top`.
pub(super) fn wide_push(value: VerificationType) -> Vec<VerificationType> {
    match value {
        VerificationType::Long | VerificationType::Double => vec![value, VerificationType::Top],
        _ => vec![value],
    }
}

/// `dup*` над значениями-категории-1: снять по позициям `take`, вернуть `put`.
pub(super) fn dup(
    state: &mut State,
    class: &str,
    method: &str,
    take: &[usize],
    put: &[usize],
) -> Result<(), ClassfileError> {
    let depth = take.iter().max().copied().unwrap_or(0);
    let taken = pop_slots(state, depth, class, method, "dup")?;
    if taken.contains(&VerificationType::Top) {
        return Err(invalid(class, method, "dup над половиной"));
    }
    let mut out = Vec::with_capacity(put.len());
    for position in put {
        out.push(taken[depth - position].clone());
    }
    push(state, out)
}

/// `dup2*`: форма выбирается по вершине (`Top` — вторая половина `long`).
pub(super) fn dup2(
    state: &mut State,
    class: &str,
    method: &str,
    deep: bool,
    deeper: bool,
) -> Result<(), ClassfileError> {
    let wide = state.stack.last() == Some(&VerificationType::Top);
    if wide {
        let depth = if deeper {
            4
        } else if deep {
            3
        } else {
            2
        };
        let taken = pop_slots(state, depth, class, method, "dup2")?;
        if taken[depth - 1] != VerificationType::Top {
            return Err(invalid(class, method, "dup2: нет половины"));
        }
        let value = taken[depth - 2].clone();
        if !matches!(value, VerificationType::Long | VerificationType::Double) {
            return Err(invalid(class, method, "dup2: половина без long/double"));
        }
        let mut out = vec![value, VerificationType::Top];
        out.extend(taken);
        push(state, out)
    } else {
        let depth = if deeper {
            4
        } else if deep {
            3
        } else {
            2
        };
        let taken = pop_slots(state, depth, class, method, "dup2")?;
        if taken.contains(&VerificationType::Top) {
            return Err(invalid(class, method, "dup2 над половиной"));
        }
        let (head, tail) = taken.split_at(depth - 2);
        let mut out = tail.to_vec();
        out.extend_from_slice(head);
        out.extend_from_slice(tail);
        push(state, out)
    }
}
