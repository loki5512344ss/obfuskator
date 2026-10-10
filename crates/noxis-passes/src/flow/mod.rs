//! Control-flow obfuscation: dispatcher prologues plus CFG rewiring.
//!
//! One scheme per method (rotation), picked from `FlowConfig::schemes`:
//! - `state-spread`: dense `tableswitch` entry dispatcher;
//! - `computed-switch`: sparse `lookupswitch` over a counter local;
//! - `butterfly`: every branch inverted, stream cut with `goto` splices;
//! - `exception-goto`: dispatcher inside `try`, dead catch-all rejoins body.
//!
//! Every scheme also cuts the stream with a few `goto` splices (intra-method
//! mixing) and recomputes `StackMapTable` afterwards, so the only thing a
//! scheme may assume is an empty stack at method entry - joins stay consistent
//! by construction (see `dispatch`/`rewire` for why each shape is safe).

mod dispatch;
mod rewire;

#[cfg(test)]
mod tests;

use dispatch::{lookup_dispatch, opaque_guard, table_dispatch};
use noxis_analysis::{ClassHierarchy, JdkHierarchy};
use noxis_classfile::{Code, Insn, Label, Program, compute_frames};
use noxis_core::{Context, CoreError, Pass, PassId, PassReport, Rng};
use rewire::{dead_catch, invert_branches, split_blocks};

/// Control-flow obfuscation pass.
#[derive(Debug, Default, Clone, Copy)]
pub struct FlowPass;

/// Dispatcher scheme (names from `FlowConfig::schemes`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scheme {
    StateSpread,
    ComputedSwitch,
    Butterfly,
    ExceptionGoto,
}

impl Scheme {
    fn parse(name: &str) -> Option<Self> {
        match name {
            "state-spread" => Some(Scheme::StateSpread),
            "computed-switch" => Some(Scheme::ComputedSwitch),
            "butterfly" => Some(Scheme::Butterfly),
            "exception-goto" => Some(Scheme::ExceptionGoto),
            _ => None,
        }
    }
}

/// Fresh-label allocator over one method body.
pub(crate) struct LabelGen {
    next: Label,
}

impl LabelGen {
    fn of(code: &Code) -> Self {
        let top = code.labels().into_iter().max().unwrap_or(0);
        Self {
            next: top.saturating_add(1),
        }
    }

    fn next(&mut self) -> Label {
        let label = self.next;
        self.next = self.next.saturating_add(1);
        label
    }
}

impl Pass for FlowPass {
    fn id(&self) -> PassId {
        PassId::Flow
    }

    fn apply(&self, ctx: &mut Context, program: &mut Program) -> Result<PassReport, CoreError> {
        let cfg = ctx.config.flow.clone();
        let mut schemes = Vec::with_capacity(cfg.schemes.len());
        for name in &cfg.schemes {
            schemes.push(
                Scheme::parse(name)
                    .ok_or_else(|| pass_error(format!("unknown scheme `{name}`")))?,
            );
        }
        if schemes.is_empty() {
            return Err(pass_error("no control-flow schemes configured"));
        }
        let mut rng = ctx.rng("flow");
        let mut mutated: Vec<(usize, usize)> = Vec::new();
        let mut per_scheme = [0usize; 4];
        let mut skipped = 0usize;

        for (ci, class) in program.classes.iter_mut().enumerate() {
            for (mi, method) in class.methods.iter_mut().enumerate() {
                let Some(code) = method.code.as_mut() else {
                    continue;
                };
                if skippable(&method.name, code, cfg.max_insns) {
                    skipped += 1;
                    continue;
                }
                let scheme = schemes[rng.below(schemes.len() as u64) as usize];
                apply_scheme(code, scheme, &cfg, &mut rng);
                per_scheme[scheme_index(scheme)] += 1;
                mutated.push((ci, mi));
            }
        }
        let hierarchy = ClassHierarchy::build(program);
        let frames = JdkHierarchy::wrap(&hierarchy);
        for (ci, mi) in &mutated {
            refresh_frames(program, *ci, *mi, &frames)?;
        }
        Ok(report(&per_scheme, mutated.len(), skipped))
    }
}

/// Whether a method body is left alone.
fn skippable(name: &str, code: &Code, max_insns: usize) -> bool {
    // The super-call-first shape of a constructor must survive: no prologue
    // may run before it.
    if name == "<init>" {
        return true;
    }
    // Frames are never computed for subroutines - keep them byte-identical.
    if code
        .insns
        .iter()
        .any(|insn| matches!(insn, Insn::Jsr(_) | Insn::Ret(_)))
    {
        return true;
    }
    let real = code
        .insns
        .iter()
        .filter(|insn| !matches!(insn, Insn::Mark(_)))
        .count();
    if real < 3 {
        return true;
    }
    if code.insns.len() > max_insns {
        return true;
    }
    false
}

/// Rebuild one method body with the given scheme; prologue stays first.
fn apply_scheme(code: &mut Code, scheme: Scheme, cfg: &noxis_core::FlowConfig, rng: &mut Rng) {
    let mut labels = LabelGen::of(code);
    let body = labels.next();
    let dead = labels.next();
    if scheme == Scheme::Butterfly {
        invert_branches(&mut code.insns, &mut labels, rng, 100);
    }
    let real = code
        .insns
        .iter()
        .filter(|insn| !matches!(insn, Insn::Mark(_)))
        .count();
    let mut prologue = Vec::new();
    if cfg.encrypt_block_bodies {
        let guard_dead = labels.next();
        let guard_merge = labels.next();
        prologue.extend(opaque_guard(guard_dead, guard_merge, rng));
    }
    match scheme {
        Scheme::StateSpread => {
            prologue.extend(table_dispatch(body, dead, rng));
            prologue.push(Insn::Mark(body));
        }
        Scheme::ComputedSwitch if code.max_locals == u16::MAX => {
            // No free local slot - fall back to the stack-based dispatcher.
            prologue.extend(table_dispatch(body, dead, rng));
            prologue.push(Insn::Mark(body));
        }
        Scheme::ComputedSwitch => {
            let slot = code.max_locals;
            prologue.extend(lookup_dispatch(body, dead, slot, rng));
            prologue.push(Insn::Mark(body));
            code.max_locals = code.max_locals.saturating_add(1);
        }
        Scheme::Butterfly => {
            prologue.push(Insn::Mark(body));
        }
        Scheme::ExceptionGoto => {
            let try_start = labels.next();
            let handler = labels.next();
            let mut guarded = vec![Insn::Mark(try_start)];
            guarded.append(&mut prologue);
            guarded.extend(table_dispatch(body, dead, rng));
            // No skip-goto after the dead block: the dispatcher's dead
            // `Goto(body)` already jumps over the handler. A second goto
            // would sit after a terminal with no frame of its own, and
            // HotSpot requires a frame at every block start - even an
            // unreachable one. try_e IS the dead mark for the same reason.
            let (mut handler_code, entry) = dead_catch(try_start, dead, handler, body);
            guarded.append(&mut handler_code);
            guarded.push(Insn::Mark(body));
            prologue = guarded;
            code.exception_table.push(entry);
        }
    }
    // A `nop` pads the body mark: the original stream may start with its own
    // mark (even a handler entry), and two marks on one offset would merge
    // frames from inconsistent states.
    prologue.push(Insn::Nop);
    let cuts = (real / 16).clamp(1, 8);
    let start = prologue.len();
    let mut insns = prologue;
    insns.append(&mut code.insns);
    split_blocks(&mut insns, &mut labels, rng, start, cuts);
    code.insns = insns;
    code.max_stack = code.max_stack.saturating_add(2);
}

fn scheme_index(scheme: Scheme) -> usize {
    match scheme {
        Scheme::StateSpread => 0,
        Scheme::ComputedSwitch => 1,
        Scheme::Butterfly => 2,
        Scheme::ExceptionGoto => 3,
    }
}

/// Recompute the frames of a touched method.
fn refresh_frames(
    program: &mut Program,
    ci: usize,
    mi: usize,
    hierarchy: &JdkHierarchy<'_>,
) -> Result<(), CoreError> {
    let (name, snapshot) = {
        let class = program
            .classes
            .get(ci)
            .ok_or_else(|| pass_error("class vanished mid-pass"))?;
        (
            class.this_class.clone(),
            class
                .methods
                .get(mi)
                .cloned()
                .ok_or_else(|| pass_error("method vanished mid-pass"))?,
        )
    };
    let frames =
        compute_frames(&name, &snapshot, hierarchy).map_err(|e| pass_error(e.to_string()))?;
    program
        .classes
        .get_mut(ci)
        .and_then(|c| c.methods.get_mut(mi))
        .and_then(|m| m.code.as_mut())
        .ok_or_else(|| pass_error("method vanished mid-pass"))?
        .frames = frames;
    Ok(())
}

fn report(per_scheme: &[usize; 4], mutated: usize, skipped: usize) -> PassReport {
    PassReport {
        pass: PassId::Flow,
        transformed: mutated,
        skipped,
        notes: vec![format!(
            "methods rebuilt: {mutated} (state-spread: {}, computed-switch: {}, butterfly: {}, exception-goto: {})",
            per_scheme[0], per_scheme[1], per_scheme[2], per_scheme[3]
        )],
    }
}

fn pass_error(msg: impl Into<String>) -> CoreError {
    CoreError::pass(PassId::Flow, msg)
}
