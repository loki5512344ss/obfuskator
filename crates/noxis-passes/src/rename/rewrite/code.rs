//! Переписывание тела метода: инструкции, кадры, handler'ы, атрибуты кода.

use noxis_classfile::{Code, Insn};
use noxis_core::{Config, Context};

use super::super::assign::{RenameMaps, rewrite_descriptor};
use super::rewrite_attributes;
use super::rewrite_frame;
use super::rewrite_method_ref;

pub(super) fn rewrite_code(
    code: &mut Code,
    maps: &RenameMaps,
    cfg: &Config,
    orig_owner: &str,
    ctx: &mut Context,
    local_counter: &mut u64,
) {
    // Имена классов в кадрах тоже переименовываются, иначе верификатор
    // увидит старые типы (`compute_frames` их не пересчитывает здесь).
    for frame in code.frames.iter_mut() {
        rewrite_frame(frame, maps);
    }
    for insn in code.insns.iter_mut() {
        match insn {
            Insn::Const(value) => {
                super::rewrite_const(value, maps);
            }
            Insn::GetStatic {
                owner,
                name,
                descriptor,
            }
            | Insn::PutStatic {
                owner,
                name,
                descriptor,
            }
            | Insn::GetField {
                owner,
                name,
                descriptor,
            }
            | Insn::PutField {
                owner,
                name,
                descriptor,
            } => {
                if let Some(new_name) = maps.field(owner, name, descriptor) {
                    *name = new_name;
                }
                if let Some(mapped) = maps.classes.get(owner.as_str()) {
                    *owner = mapped.clone();
                }
                *descriptor = rewrite_descriptor(descriptor, &maps.classes);
            }
            Insn::Invoke {
                owner,
                name,
                descriptor,
                ..
            } => {
                rewrite_method_ref(owner, name, descriptor, maps);
            }
            // The call-site name belongs to the functional interface and
            // stays; the descriptor (captured receiver types) is rewritten,
            // and the bootstrap table entry is handled with the attributes.
            Insn::InvokeDynamic { descriptor, .. } => {
                *descriptor = rewrite_descriptor(descriptor, &maps.classes);
            }
            Insn::New(name) => {
                if let Some(mapped) = maps.classes.get(name.as_str()) {
                    *name = mapped.clone();
                }
            }
            // Type operands may be array descriptors (`[Lpkg/Cls;`, `[B`):
            // the element class inside follows the mapping, while a plain
            // name takes the exact entry. A stale descriptor is worse than
            // useless - later frame merges treat it as a ghost supertype.
            Insn::CheckCast(name) | Insn::InstanceOf(name) | Insn::ANewArray(name) => {
                if name.starts_with('[') {
                    *name = rewrite_descriptor(name, &maps.classes);
                } else if let Some(mapped) = maps.classes.get(name.as_str()) {
                    *name = mapped.clone();
                }
            }
            Insn::MultiANewArray { descriptor, .. } => {
                *descriptor = rewrite_descriptor(descriptor, &maps.classes);
            }
            _ => {}
        }
    }

    for handler in code.exception_table.iter_mut() {
        if let Some(catch) = handler.catch_type.clone()
            && let Some(mapped) = maps.classes.get(catch.as_str())
        {
            handler.catch_type = Some(mapped.clone());
        }
    }

    rewrite_attributes(
        &mut code.attributes,
        maps,
        cfg,
        orig_owner,
        ctx,
        local_counter,
    );
}
