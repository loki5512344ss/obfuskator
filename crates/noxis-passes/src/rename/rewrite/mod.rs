//! Переписывание одного класса: имена, дескрипторы, код, атрибуты.

mod code;

use noxis_classfile::{Attribute, ClassFile, StackMapFrame, VerificationType};
use noxis_core::{Config, Context};

use super::assign::{RenameMaps, rewrite_descriptor};
use crate::namegen::NameGenerator;
use code::rewrite_code;

pub(super) fn rewrite_class(
    class: &mut ClassFile,
    orig_owner: &str,
    maps: &RenameMaps,
    cfg: &Config,
    local_counter: &mut u64,
    ctx: &mut Context,
) {
    if let Some(new_name) = maps.classes.get(orig_owner) {
        class.this_class = new_name.clone();
    }
    if let Some(mapped) = class
        .super_class
        .as_ref()
        .and_then(|sup| maps.classes.get(sup))
        .cloned()
    {
        class.super_class = Some(mapped);
    }
    for iface in &mut class.interfaces {
        if let Some(mapped) = maps.classes.get(iface.as_str()) {
            *iface = mapped.clone();
        }
    }

    for field in &mut class.fields {
        let key = (
            orig_owner.to_string(),
            field.name.clone(),
            field.descriptor.clone(),
        );
        if let Some(new_name) = maps.fields.get(&key) {
            field.name = new_name.clone();
        }
        field.descriptor = rewrite_descriptor(&field.descriptor, &maps.classes);
        rewrite_attributes(
            &mut field.attributes,
            maps,
            cfg,
            orig_owner,
            ctx,
            local_counter,
        );
    }

    for method in &mut class.methods {
        let key = (
            orig_owner.to_string(),
            method.name.clone(),
            method.descriptor.clone(),
        );
        if let Some(new_name) = maps.methods.get(&key) {
            method.name = new_name.clone();
        }
        method.descriptor = rewrite_descriptor(&method.descriptor, &maps.classes);
        if let Some(code) = &mut method.code {
            rewrite_code(code, maps, cfg, orig_owner, ctx, local_counter);
        }
        rewrite_attributes(
            &mut method.attributes,
            maps,
            cfg,
            orig_owner,
            ctx,
            local_counter,
        );
    }

    rewrite_attributes(
        &mut class.attributes,
        maps,
        cfg,
        orig_owner,
        ctx,
        local_counter,
    );
}

fn rewrite_attributes(
    attributes: &mut [Attribute],
    maps: &RenameMaps,
    cfg: &Config,
    orig_owner: &str,
    ctx: &mut Context,
    local_counter: &mut u64,
) {
    for attr in attributes.iter_mut() {
        match attr {
            Attribute::Signature(sig) => {
                *sig = rewrite_descriptor(sig, &maps.classes);
            }
            Attribute::Exceptions(list) => {
                for name in list {
                    if let Some(mapped) = maps.classes.get(name.as_str()) {
                        *name = mapped.clone();
                    }
                }
            }
            Attribute::InnerClasses(inner) => {
                for entry in inner.iter_mut() {
                    if let Some(mapped) = maps.classes.get(entry.inner.as_str()) {
                        entry.inner = mapped.clone();
                    }
                    if let Some(mapped) = entry
                        .outer
                        .as_ref()
                        .and_then(|outer| maps.classes.get(outer))
                        .cloned()
                    {
                        entry.outer = Some(mapped);
                    }
                }
            }
            Attribute::LocalVariableTable(vars) => {
                for var in vars.iter_mut() {
                    var.descriptor = rewrite_descriptor(&var.descriptor, &maps.classes);
                    if cfg.rename.rename_locals {
                        *local_counter += 1;
                        let mut names = NameGenerator::new(
                            ctx.rng_nth("rename/locals", *local_counter),
                            cfg.rename.local_style,
                        );
                        let new = names.next_name();
                        ctx.mapping.locals.insert(
                            format!("{orig_owner}/{}@{}", var.name, var.index),
                            new.clone(),
                        );
                        var.name = new;
                    }
                }
            }
            _ => {}
        }
    }
}

/// Имена классов внутри одного кадра (`Object(..)` в locals/stack).
fn rewrite_frame(frame: &mut StackMapFrame, maps: &RenameMaps) {
    match frame {
        StackMapFrame::Same { .. } | StackMapFrame::Chop { .. } => {}
        StackMapFrame::SameLocals { local, .. } => rewrite_type(local, maps),
        StackMapFrame::Append { locals, .. } => rewrite_types(locals, maps),
        StackMapFrame::Full { locals, stack, .. } => {
            rewrite_types(locals, maps);
            rewrite_types(stack, maps);
        }
    }
}

fn rewrite_types(types: &mut [VerificationType], maps: &RenameMaps) {
    for item in types.iter_mut() {
        rewrite_type(item, maps);
    }
}

fn rewrite_type(item: &mut VerificationType, maps: &RenameMaps) {
    if let VerificationType::Object(name) = item
        && let Some(mapped) = maps.classes.get(name.as_str())
    {
        *name = mapped.clone();
    }
}
