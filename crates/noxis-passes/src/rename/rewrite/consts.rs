//! Constant-pool references: owners and descriptors follow the mapping.

use noxis_classfile::Const;

use super::super::assign::{RenameMaps, rewrite_descriptor};

/// A method reference (invoke/bootstrap/method-handle): the owner follows
/// class renames, the name follows member renames, the descriptor is
/// rewritten. Array owners (`[Lpkg/C;` - e.g. `clone` on arrays) name the
/// element class inside a descriptor, so they take the descriptor path.
pub(super) fn rewrite_method_ref(
    owner: &mut String,
    name: &mut String,
    descriptor: &mut String,
    maps: &RenameMaps,
) {
    if owner.starts_with('[') {
        *owner = rewrite_descriptor(owner, &maps.classes);
    } else {
        if let Some(new_name) = maps.method(owner, name, descriptor) {
            *name = new_name;
        }
        if let Some(mapped) = maps.classes.get(owner.as_str()) {
            *owner = mapped.clone();
        }
    }
    *descriptor = rewrite_descriptor(descriptor, &maps.classes);
}

/// A constant-pool reference inside code or bootstrap arguments: class
/// literals (plain or array), method types, method handles, condy payloads.
/// Anything else (numbers, strings) has no references to follow.
pub(super) fn rewrite_const(value: &mut Const, maps: &RenameMaps) {
    match value {
        Const::Class(name) => {
            if name.starts_with('[') {
                *name = rewrite_descriptor(name, &maps.classes);
            } else if let Some(mapped) = maps.classes.get(name.as_str()) {
                *name = mapped.clone();
            }
        }
        Const::MethodType(descriptor) => {
            *descriptor = rewrite_descriptor(descriptor, &maps.classes);
        }
        Const::MethodHandle {
            owner,
            name,
            descriptor,
            ..
        } => {
            rewrite_method_ref(owner, name, descriptor, maps);
        }
        Const::Dynamic { descriptor, .. } => {
            *descriptor = rewrite_descriptor(descriptor, &maps.classes);
        }
        _ => {}
    }
}
