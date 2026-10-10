//! Weaving junk classes into the reachability graph of live methods.

use noxis_analysis::{ClassHierarchy, JdkHierarchy};
use noxis_classfile::{Insn, InvokeKind, Program, compute_frames};
use noxis_core::{CoreError, PassId};

/// The woven call descriptor (entries are always `()V`).
const WOVEN_DESCRIPTOR: &str = "()V";

/// Weave a call of the first junk entry into the first method with code.
///
/// The call lands before the terminal (`Return`/`AReturn`/`AThrow`) so it
/// never strands an unreachable tail without frames; frames are recomputed.
pub(super) fn weave_first_reachable(
    program: &mut Program,
    real_classes: usize,
    entries: &[(String, String)],
) -> Result<usize, CoreError> {
    let missing = || CoreError::pass(PassId::Noise, "weave target vanished");
    let Some((owner, entry)) = entries.first() else {
        return Err(missing());
    };
    let target = program.classes[..real_classes.min(program.classes.len())]
        .iter()
        .enumerate()
        .find_map(|(ci, c)| {
            c.methods
                .iter()
                .position(|m| m.code.is_some())
                .map(|mi| (ci, mi))
        });
    let Some((ci, mi)) = target else { return Ok(0) };

    let class_name = program
        .classes
        .get(ci)
        .ok_or_else(missing)?
        .this_class
        .clone();
    let call = Insn::Invoke {
        kind: InvokeKind::Static,
        owner: owner.clone(),
        name: entry.clone(),
        descriptor: WOVEN_DESCRIPTOR.to_string(),
        itf: false,
    };
    {
        let code = program
            .classes
            .get_mut(ci)
            .and_then(|c| c.methods.get_mut(mi))
            .and_then(|m| m.code.as_mut())
            .ok_or_else(missing)?;
        if matches!(
            code.insns.last(),
            Some(Insn::Return(_) | Insn::AReturn | Insn::AThrow)
        ) {
            code.insns.insert(code.insns.len() - 1, call);
        } else {
            code.insns.push(call);
        }
    }

    let snapshot = program
        .classes
        .get(ci)
        .and_then(|c| c.methods.get(mi))
        .cloned()
        .ok_or_else(missing)?;
    let hierarchy = ClassHierarchy::build(program);
    let frames_hierarchy = JdkHierarchy::wrap(&hierarchy);
    let frames = compute_frames(&class_name, &snapshot, &frames_hierarchy)
        .map_err(|e| CoreError::pass(PassId::Noise, e.to_string()))?;
    program
        .classes
        .get_mut(ci)
        .and_then(|c| c.methods.get_mut(mi))
        .and_then(|m| m.code.as_mut())
        .ok_or_else(missing)?
        .frames = frames;
    Ok(1)
}
