//! `StackMapTable`: представление фреймов и вычисление после трансформаций.
//!
//! Фреймы — самая дорогая часть любого обфускатора на Rust: JVM-верификатор
//! (class version ≥ 50) требует точного `StackMapTable`, иначе класс не грузится.
//! Варианты реализации и их цены — `docs/stack.md`.
//!
//! Метод: CFG по меткам → worklist с типами (`xfer`) → слияние LUB (`merge`)
//! → эмиссия компактных кадров (`emit`). Байтовые смещения берутся тем же
//! преобразованием, что использует writer, — иначе кадры разъедутся с байтами.

mod emit;
mod flow;
mod merge;
mod xfer;

#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, VecDeque};

use crate::error::ClassfileError;
use crate::insn::{Insn, Label, Prim};
use crate::model::Method;

use flow::{HandlerRange, frame_targets, handler_ranges, strip_marks, successors};
pub use merge::Hierarchy;
use merge::Merger;
use xfer::{State, transfer};

/// Тип верификации из JVMS §4.10.1.
#[derive(Debug, Clone, PartialEq)]
pub enum VerificationType {
    Top,
    Integer,
    Float,
    Double,
    Long,
    Null,
    UninitializedThis,
    /// Внутреннее имя класса (`java/lang/Object`) или дескриптор массива.
    Object(String),
    /// Байтовое смещение инструкции `new`.
    Uninitialized(u16),
}

impl VerificationType {
    /// Тип, соответствующий примитиву.
    pub fn of_prim(p: Prim) -> Self {
        match p {
            Prim::Int => VerificationType::Integer,
            Prim::Float => VerificationType::Float,
            Prim::Long => VerificationType::Long,
            Prim::Double => VerificationType::Double,
        }
    }
}

/// Запись `StackMapTable` (JVMS §4.7.4).
#[derive(Debug, Clone, PartialEq)]
pub enum StackMapFrame {
    Same {
        offset_delta: u32,
    },
    SameLocals {
        offset_delta: u32,
        local: VerificationType,
    },
    Chop {
        offset_delta: u32,
        count: u8,
    },
    Append {
        offset_delta: u32,
        locals: Vec<VerificationType>,
    },
    Full {
        offset_delta: u32,
        locals: Vec<VerificationType>,
        stack: Vec<VerificationType>,
    },
}

/// Вычислить фреймы для тела метода после всех трансформаций.
///
/// Кадры нужны только классам версии ≥ 50; `jsr`/`ret` отвергаются (такие
/// классы старше и кадры им не нужны). Иерархия — для LUB ссылочных типов.
pub fn compute_frames(
    class: &str,
    method: &Method,
    hierarchy: &dyn Hierarchy,
) -> Result<Vec<StackMapFrame>, ClassfileError> {
    let code = method
        .code
        .as_ref()
        .ok_or_else(|| invalid(class, &method.name, "нет тела"))?;
    if code.insns.is_empty() {
        return Ok(Vec::new());
    }
    let merger = Merger {
        class: class.to_string(),
        method: method.name.clone(),
        hierarchy,
    };
    let (labels, bare) = strip_marks(&code.insns, &merger)?;
    let bytes = layout(&bare, &labels, &merger)?;
    let handlers = handler_ranges(&code.exception_table, &labels, &merger)?;
    let initial = merger.initial(method, code.max_locals)?;
    let mut states: Vec<Option<State>> = vec![None; bare.len()];
    states[0] = Some(initial.clone());
    run_worklist(&merger, &bare, &bytes, &labels, &handlers, &mut states)?;
    let targets = frame_targets(&bare, &labels, &handlers, &merger)?;
    emit::emit_frames(&states, &targets, &initial.locals, &merger)
}

/// Байтовое смещение каждой инструкции — тем же кодированием, что в writer'е.
fn layout(
    bare: &[&Insn],
    labels: &BTreeMap<Label, u32>,
    merger: &Merger<'_>,
) -> Result<Vec<u16>, ClassfileError> {
    use crate::format::write::{code, pool::Pool};

    let mut pool = Pool::new();
    let resolve = |label: Label| -> Result<u32, ClassfileError> {
        labels
            .get(&label)
            .copied()
            .ok_or_else(|| merger.invalid(&format!("нет метки {label}")))
    };
    let mut rt_insns = Vec::with_capacity(bare.len());
    for (index, insn) in bare.iter().enumerate() {
        let index = u32::try_from(index).map_err(|_| merger.invalid("слишком много инструкций"))?;
        if let Some(rt) = code::insns::convert(insn, index, &mut pool, &resolve)? {
            rt_insns.push(rt);
        }
    }
    let map = code::measure(&rt_insns)?;
    (0..bare.len())
        .map(|index| {
            map.get(&(index as u32))
                .copied()
                .ok_or_else(|| merger.invalid("замер потерял инструкцию"))
        })
        .collect()
}

/// Основной цикл: состояния текут по рёбрам CFG до неподвижной точки.
fn run_worklist(
    merger: &Merger<'_>,
    bare: &[&Insn],
    bytes: &[u16],
    labels: &BTreeMap<Label, u32>,
    handlers: &[HandlerRange],
    states: &mut [Option<State>],
) -> Result<(), ClassfileError> {
    let mut queue: VecDeque<u32> = VecDeque::from([0]);
    let mut fuel = 1_000_000u32;
    while let Some(index) = queue.pop_front() {
        let at = index as usize;
        let state = states[at]
            .clone()
            .ok_or_else(|| merger.invalid("пустое состояние"))?;
        let mut out = state;
        transfer(&merger.class, &merger.method, bare[at], bytes[at], &mut out).map_err(
            |error| match error {
                ClassfileError::InvalidCode { class, msg } => ClassfileError::InvalidCode {
                    class,
                    msg: format!("[{at}] {msg}"),
                },
                other => other,
            },
        )?;
        let nexts = successors(bare[at], index, bare.len() as u32, labels, merger)?;
        for next in nexts {
            flow_into(merger, states, &mut queue, &mut fuel, next, out.clone())?;
        }
        for (start, end, handler, catch) in handlers {
            if index >= *start && index < *end {
                let caught = State {
                    locals: out.locals.clone(),
                    stack: vec![catch_type(catch)],
                };
                flow_into(merger, states, &mut queue, &mut fuel, *handler, caught)?;
            }
        }
    }
    Ok(())
}

/// Влить состояние в инструкцию; `true` — изменилось, продолжить обход.
fn flow_into(
    merger: &Merger<'_>,
    states: &mut [Option<State>],
    queue: &mut VecDeque<u32>,
    fuel: &mut u32,
    next: u32,
    state: State,
) -> Result<(), ClassfileError> {
    if *fuel == 0 {
        return Err(merger.invalid("нет неподвижной точки"));
    }
    *fuel -= 1;
    let slot = states
        .get_mut(next as usize)
        .ok_or_else(|| merger.invalid("переход вне тела метода"))?;
    let changed = merger
        .merge_into(slot, state)
        .map_err(|error| match error {
            ClassfileError::InvalidCode { class, msg } => ClassfileError::InvalidCode {
                class,
                msg: format!("[->{next}] {msg}"),
            },
            other => other,
        })?;
    if changed {
        queue.push_back(next);
    }
    Ok(())
}

/// Рёбра CFG без исключений: продолжение + цели переходов.
/// Тип исключения на входе в handler (`catch-all` — любое throwable).
fn catch_type(catch: &Option<String>) -> VerificationType {
    match catch {
        Some(name) => VerificationType::Object(name.clone()),
        None => VerificationType::Object("java/lang/Throwable".to_string()),
    }
}

fn invalid(class: &str, method: &str, msg: &str) -> ClassfileError {
    ClassfileError::InvalidCode {
        class: class.to_string(),
        msg: format!("{method}: {msg}"),
    }
}
