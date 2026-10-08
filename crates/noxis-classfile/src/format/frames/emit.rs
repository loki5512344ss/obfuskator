//! Эмиссия компактных кадров в точках, где их ждёт верификатор.
//!
//! Вид кадра выбирается сравнением с предыдущим кадром (`Same`/`Append`/
//! `Chop`/`SameLocals`, иначе `Full`) — как у `javac`. Хвостовые `Top`
//! при сравнении и записи отрезаются: пустой слот — не тип.

use std::collections::BTreeSet;

use super::merge::Merger;
use super::xfer::State;
use super::{StackMapFrame, VerificationType};
use crate::error::ClassfileError;

/// Кадры во всех точках `targets` (отсортированы — дельты по порядку).
///
/// Дельты — в номерах инструкций (как backend после чтения); writer сам
/// пересчитает их в байты.
pub(super) fn emit_frames(
    states: &[Option<State>],
    targets: &BTreeSet<u32>,
    initial: &[VerificationType],
    merger: &Merger<'_>,
) -> Result<Vec<StackMapFrame>, ClassfileError> {
    let mut out = Vec::new();
    let mut previous_locals = initial.to_vec();
    let mut previous_index: Option<u32> = None;
    for target in targets {
        let at = *target as usize;
        let Some(state) = &states[at] else {
            continue;
        };
        let delta = match previous_index {
            None => *target,
            Some(previous) => target
                .checked_sub(previous)
                .and_then(|delta| delta.checked_sub(1))
                .ok_or_else(|| merger.invalid("кадры не по порядку"))?,
        };
        out.push(select(delta, &state.locals, &previous_locals, &state.stack));
        previous_locals = state.locals.clone();
        previous_index = Some(*target);
    }
    Ok(out)
}

fn select(
    delta: u32,
    locals: &[VerificationType],
    previous: &[VerificationType],
    stack: &[VerificationType],
) -> StackMapFrame {
    // Сравнение — на гранулярности переменных (`long` — одна запись,
    // как у `javac`): иначе половины пар дают `Append{[Top, Long]}`.
    let current = vars(locals);
    let base = vars(previous);
    if current == base {
        if stack.is_empty() {
            return StackMapFrame::Same {
                offset_delta: delta,
            };
        }
        if let [local] = stack {
            return StackMapFrame::SameLocals {
                offset_delta: delta,
                local: local.clone(),
            };
        }
    }
    if stack.is_empty()
        && let Some(added) = current.strip_prefix(base.as_slice())
        && (1..=3).contains(&added.len())
        && !added.contains(&VerificationType::Top)
    {
        return StackMapFrame::Append {
            offset_delta: delta,
            locals: added.to_vec(),
        };
    }
    if stack.is_empty()
        && let Some(dropped) = base.strip_prefix(current.as_slice())
        && (1..=3).contains(&dropped.len())
        && let Ok(count) = u8::try_from(dropped.len())
    {
        return StackMapFrame::Chop {
            offset_delta: delta,
            count,
        };
    }
    StackMapFrame::Full {
        offset_delta: delta,
        locals: current,
        stack: stack.to_vec(),
    }
}

/// Слоты → переменные: `[Long, Top]` — одна запись; хвостовые `Top` — прочь.
///
/// Одиночные `Top` внутри (дыры) сохраняются: в `Full` это легальный тип,
/// а в `Append` мы их не пускаем (см. `select`).
fn vars(locals: &[VerificationType]) -> Vec<VerificationType> {
    let mut out = Vec::with_capacity(locals.len());
    let mut index = 0;
    while index < locals.len() {
        match &locals[index] {
            VerificationType::Long | VerificationType::Double
                if locals.get(index + 1) == Some(&VerificationType::Top) =>
            {
                out.push(locals[index].clone());
                index += 2;
            }
            other => {
                out.push(other.clone());
                index += 1;
            }
        }
    }
    while out.last() == Some(&VerificationType::Top) {
        out.pop();
    }
    out
}
