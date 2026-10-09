//! The constants pass: numbers become arithmetic, plus opaque predicates.
//!
//! `float`/`double` are left alone (bit-exact paths through integers are
//! separate). Trivia from the `iconst` range (`-1..=5`, `0L/1L`) stays as is:
//! encrypting it only bloats hot code.

mod forms;

#[cfg(test)]
mod tests;

use noxis_analysis::ClassHierarchy;
use noxis_classfile::{Const, Insn, MAX_METHOD_INSNS, Program, compute_frames};
use noxis_core::{Context, CoreError, Pass, PassId, PassReport};

use forms::{Scheme, encrypt_int, encrypt_long, opaque_true};

/// The constants encryption pass.
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
                    .ok_or_else(|| pass_error(format!("unknown scheme `{name}`")))?,
            );
        }
        if schemes.is_empty() {
            return Err(pass_error("no encryption schemes"));
        }
        let mut rng = ctx.rng("constants");
        let mut replaced = 0usize;
        let mut predicates = 0usize;
        let mut skipped = 0usize;
        let mut mutated: Vec<(usize, usize)> = Vec::new();
        let mut oversized: Vec<(usize, usize)> = Vec::new();

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
                let original = std::mem::take(&mut code.insns);
                let mut fresh = Vec::with_capacity(original.len());
                let mut touched = false;
                for insn in &original {
                    match insn {
                        Insn::Const(Const::Int(value)) if !is_tiny_int(*value) => {
                            let scheme = schemes[rng.below(schemes.len() as u64) as usize];
                            fresh.extend(encrypt_int(*value, scheme, &mut rng));
                            replaced += 1;
                            touched = true;
                        }
                        Insn::Const(Const::Long(value)) if *value != 0 && *value != 1 => {
                            let scheme = schemes[rng.below(schemes.len() as u64) as usize];
                            fresh.extend(encrypt_long(*value, scheme, &mut rng));
                            replaced += 1;
                            touched = true;
                        }
                        other => fresh.push(other.clone()),
                    }
                }
                if fresh.len() > MAX_METHOD_INSNS {
                    // Would risk the 64KB writer ceiling: keep the original
                    // code untouched (predicates included) and count a skip.
                    code.insns = original;
                    skipped += 1;
                    oversized.push((ci, mi));
                    continue;
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
            &oversized,
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

/// Trivia that is not worth encrypting (already `iconst`).
fn is_tiny_int(value: i32) -> bool {
    (-1..=5).contains(&value)
}

/// Opaque predicates at the top of methods with code (at most the limit).
/// Oversized methods stay fully untouched, predicates included.
fn insert_predicates(
    program: &mut Program,
    mutated: &mut Vec<(usize, usize)>,
    oversized: &[(usize, usize)],
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
            if oversized.contains(&(ci, mi)) {
                continue;
            }
            let empty_or_missing = program.classes[ci].methods[mi]
                .code
                .as_ref()
                .is_none_or(|code| code.insns.is_empty());
            if empty_or_missing {
                continue;
            }
            let missing = || pass_error("method vanished while inserting a predicate");
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

/// Recompute the frames of a touched method.
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
            .ok_or_else(|| pass_error("class vanished"))?;
        (
            class.this_class.clone(),
            class
                .methods
                .get(mi)
                .cloned()
                .ok_or_else(|| pass_error("method vanished"))?,
        )
    };
    let frames =
        compute_frames(&name, &snapshot, hierarchy).map_err(|e| pass_error(e.to_string()))?;
    program
        .classes
        .get_mut(ci)
        .and_then(|c| c.methods.get_mut(mi))
        .and_then(|m| m.code.as_mut())
        .ok_or_else(|| pass_error("method vanished"))?
        .frames = frames;
    Ok(())
}

fn report(replaced: usize, predicates: usize, skipped: usize) -> PassReport {
    PassReport {
        pass: PassId::Constants,
        transformed: replaced + predicates,
        skipped,
        notes: vec![format!(
            "constants split: {replaced}, predicates: {predicates}"
        )],
    }
}

fn pass_error(msg: impl Into<String>) -> CoreError {
    CoreError::pass(PassId::Constants, msg)
}
