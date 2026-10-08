//! Вплетание junk-классов в граф достижимости живых методов.

use noxis_analysis::ClassHierarchy;
use noxis_classfile::{Insn, InvokeKind, Program, compute_frames};
use noxis_core::{CoreError, PassId};

use super::JUNK_ENTRY;

/// Вплетает вызов первого junk-класса в первый метод с кодом.
///
/// Вызов встаёт перед терминалом (`Return`/`AReturn`/`AThrow`), чтобы не
/// породить недостижимый хвост без фреймов; фреймы пересчитываются.
pub(super) fn weave_first_reachable(
    program: &mut Program,
    real_classes: usize,
    junk_names: &[String],
) -> Result<usize, CoreError> {
    let missing = || CoreError::pass(PassId::Noise, "цель вплетания пропала");
    let Some(owner) = junk_names.first() else {
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
        name: JUNK_ENTRY.0.to_string(),
        descriptor: JUNK_ENTRY.1.to_string(),
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
    let frames = compute_frames(&class_name, &snapshot, &hierarchy)
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
