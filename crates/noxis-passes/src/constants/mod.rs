//! Пасс шифрования констант: числа → арифметика, плюс opaque-предикаты.
//!
//! `float`/`double` не трогаются (битовая точность через целые — отдельно).
//! Мелочь из `iconst`-диапазона (`-1..=5`, `0L/1L`) остаётся как есть:
//! её шифрование только раздувает горячий код.

mod forms;

#[cfg(test)]
mod tests;

use noxis_analysis::ClassHierarchy;
use noxis_classfile::{Const, Insn, Program, compute_frames};
use noxis_core::{Context, CoreError, Pass, PassId, PassReport};

use forms::{Scheme, encrypt_int, encrypt_long, opaque_true};

/// Пасс шифрования констант.
#[derive(Debug, Default, Clone, Copy)]
pub struct ConstantsPass;

impl Pass for ConstantsPass {
    fn id(&self) -> PassId {
        PassId::Constants
    }

    fn apply(&self, ctx: &mut Context, program: &mut Program) -> Result<PassReport, CoreError> {
        let cfg = ctx.config.constants.clone();
        let mut schemes = Vec::with_capacity(cfg.schemes.len());
        for name in &cfg.schemes {
            schemes.push(
                Scheme::parse(name)
                    .ok_or_else(|| pass_error(format!("неизвестная схема `{name}`")))?,
            );
        }
        if schemes.is_empty() {
            return Err(pass_error("нет схем шифрования"));
        }
        let mut rng = ctx.rng("constants");
        let mut replaced = 0usize;
        let mut predicates = 0usize;
        let mut skipped = 0usize;
        let mut mutated: Vec<(usize, usize)> = Vec::new();

        for (ci, class) in program.classes.iter_mut().enumerate() {
            if cfg
                .skip
                .iter()
                .any(|prefix| class.this_class.starts_with(prefix))
            {
                skipped += 1;
                continue;
            }
            for (mi, method) in class.methods.iter_mut().enumerate() {
                let Some(code) = method.code.as_mut() else {
                    continue;
                };
                let mut fresh = Vec::with_capacity(code.insns.len());
                let mut touched = false;
                for insn in code.insns.drain(..) {
                    match insn {
                        Insn::Const(Const::Int(value)) if !is_tiny_int(value) => {
                            let scheme = schemes[rng.below(schemes.len() as u64) as usize];
                            fresh.extend(encrypt_int(value, scheme, &mut rng));
                            replaced += 1;
                            touched = true;
                        }
                        Insn::Const(Const::Long(value)) if value != 0 && value != 1 => {
                            let scheme = schemes[rng.below(schemes.len() as u64) as usize];
                            fresh.extend(encrypt_long(value, scheme, &mut rng));
                            replaced += 1;
                            touched = true;
                        }
                        other => fresh.push(other),
                    }
                }
                code.insns = fresh;
                if touched {
                    code.max_stack = code.max_stack.saturating_add(2);
                    mutated.push((ci, mi));
                }
            }
        }
        insert_predicates(
            program,
            &mut mutated,
            cfg.max_predicates,
            &mut rng,
            &mut predicates,
        )?;
        let hierarchy = ClassHierarchy::build(program);
        for (ci, mi) in &mutated {
            refresh_frames(program, *ci, *mi, &hierarchy)?;
        }
        Ok(report(replaced, predicates, skipped))
    }
}

/// Мелочь, которую шифровать невыгодно (уже `iconst`).
fn is_tiny_int(value: i32) -> bool {
    (-1..=5).contains(&value)
}

/// Opaque-предикаты в начало методов с кодом (не больше лимита на метод).
fn insert_predicates(
    program: &mut Program,
    mutated: &mut Vec<(usize, usize)>,
    max_predicates: usize,
    rng: &mut noxis_core::Rng,
    predicates: &mut usize,
) -> Result<(), CoreError> {
    if max_predicates == 0 {
        return Ok(());
    }
    let classes = program.classes.len();
    for ci in 0..classes {
        let methods = program.classes[ci].methods.len();
        for mi in 0..methods {
            let empty_or_missing = program.classes[ci].methods[mi]
                .code
                .as_ref()
                .is_none_or(|code| code.insns.is_empty());
            if empty_or_missing {
                continue;
            }
            let missing = || pass_error("метод пропал при вставке предиката");
            let mut top = program.classes[ci]
                .methods
                .get(mi)
                .and_then(|method| method.code.as_ref())
                .ok_or_else(missing)?
                .labels()
                .into_iter()
                .max()
                .unwrap_or(0);
            for _ in 0..max_predicates {
                top += 1;
                let dead = top;
                top += 1;
                let merge = top;
                let at = program.classes[ci]
                    .methods
                    .get(mi)
                    .and_then(|method| method.code.as_ref())
                    .ok_or_else(missing)?
                    .insns
                    .iter()
                    .position(|insn| !matches!(insn, Insn::Mark(_)))
                    .unwrap_or(0);
                let code = program.classes[ci]
                    .methods
                    .get_mut(mi)
                    .and_then(|method| method.code.as_mut())
                    .ok_or_else(missing)?;
                let mut block = opaque_true(dead, merge, rng);
                block.extend(code.insns.drain(at..));
                code.insns.splice(at..at, block);
                code.max_stack = code.max_stack.saturating_add(2);
                *predicates += 1;
            }
            if !mutated.contains(&(ci, mi)) {
                mutated.push((ci, mi));
            }
        }
    }
    Ok(())
}

/// Пересчитать фреймы затронутого метода.
fn refresh_frames(
    program: &mut Program,
    ci: usize,
    mi: usize,
    hierarchy: &ClassHierarchy,
) -> Result<(), CoreError> {
    let (name, snapshot) = {
        let class = program
            .classes
            .get(ci)
            .ok_or_else(|| pass_error("класс пропал"))?;
        (
            class.this_class.clone(),
            class
                .methods
                .get(mi)
                .cloned()
                .ok_or_else(|| pass_error("метод пропал"))?,
        )
    };
    let frames =
        compute_frames(&name, &snapshot, hierarchy).map_err(|e| pass_error(e.to_string()))?;
    program
        .classes
        .get_mut(ci)
        .and_then(|c| c.methods.get_mut(mi))
        .and_then(|m| m.code.as_mut())
        .ok_or_else(|| pass_error("метод пропал"))?
        .frames = frames;
    Ok(())
}

fn report(replaced: usize, predicates: usize, skipped: usize) -> PassReport {
    PassReport {
        pass: PassId::Constants,
        transformed: replaced + predicates,
        skipped,
        notes: vec![format!(
            "констант разбито: {replaced}, предикатов: {predicates}"
        )],
    }
}

fn pass_error(msg: impl Into<String>) -> CoreError {
    CoreError::pass(PassId::Constants, msg)
}
