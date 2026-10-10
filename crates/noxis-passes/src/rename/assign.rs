//! Assigning new names: maps, virtual method families, the package pool,
//! and final access widening.

use std::collections::{BTreeMap, BTreeSet};

use noxis_analysis::{Analysis, ClassHierarchy};
use noxis_classfile::Program;
use noxis_core::{Config, Context, NameStyle, Rng};

use super::{ACC_PRIVATE, ACC_PROTECTED, ACC_PUBLIC, ACC_STATIC};
use crate::namegen::NameGenerator;

/// `(owner, name, descriptor)` - matches a JVM reference.
pub(super) type MemberKey = (String, String, String);

/// Rename maps for one run: `declared` is where a member was declared,
/// `alias` is subclasses that inherit the name without declaring it themselves.
#[derive(Debug, Default)]
pub(super) struct RenameMaps {
    pub(super) classes: BTreeMap<String, String>,
    pub(super) methods: BTreeMap<MemberKey, String>,
    pub(super) methods_alias: BTreeMap<MemberKey, String>,
    pub(super) fields: BTreeMap<MemberKey, String>,
    pub(super) fields_alias: BTreeMap<MemberKey, String>,
}

impl RenameMaps {
    pub(super) fn method(&self, owner: &str, name: &str, desc: &str) -> Option<String> {
        let key = (owner.to_string(), name.to_string(), desc.to_string());
        self.methods
            .get(&key)
            .or_else(|| self.methods_alias.get(&key))
            .cloned()
    }

    pub(super) fn field(&self, owner: &str, name: &str, desc: &str) -> Option<String> {
        let key = (owner.to_string(), name.to_string(), desc.to_string());
        self.fields
            .get(&key)
            .or_else(|| self.fields_alias.get(&key))
            .cloned()
    }

    /// Aliases: a subclass calls a member declared on a parent class.
    pub(super) fn expand(&mut self, hierarchy: &ClassHierarchy, owners: &[String]) {
        let methods: Vec<(MemberKey, String)> = self
            .methods
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        for ((owner, name, desc), new_name) in &methods {
            for child in owners {
                if child == owner || !hierarchy.is_subclass_of(child, owner) {
                    continue;
                }
                let key = (child.clone(), name.clone(), desc.clone());
                if self.methods.contains_key(&key) {
                    continue;
                }
                self.methods_alias
                    .entry(key)
                    .or_insert_with(|| new_name.clone());
            }
        }

        let fields: Vec<(MemberKey, String)> = self
            .fields
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        for ((owner, name, desc), new_name) in &fields {
            for child in owners {
                if child == owner || !hierarchy.is_subclass_of(child, owner) {
                    continue;
                }
                let key = (child.clone(), name.clone(), desc.clone());
                if self.fields.contains_key(&key) {
                    continue;
                }
                self.fields_alias
                    .entry(key)
                    .or_insert_with(|| new_name.clone());
            }
        }
    }
}

/// Rename virtual methods as a "family": one name for the whole hierarchy,
/// including bridge methods with covariant returns.
pub(super) fn rename_virtual_families(
    program: &Program,
    analysis: &Analysis,
    maps: &mut RenameMaps,
    cfg: &Config,
    ctx: &mut Context,
) -> usize {
    // Family = (name, params): the return type is ignored, so covariant
    // returns and bridge methods (`ACC_BRIDGE`) are captured together.
    let mut families: BTreeMap<(String, String), Vec<MemberKey>> = BTreeMap::new();
    let mut locked_families: BTreeSet<(String, String)> = BTreeSet::new();

    for class in &program.classes {
        let locked_class = analysis.is_locked(&class.this_class);
        for method in &class.methods {
            if method.name == "<init>" || method.name == "<clinit>" {
                continue;
            }
            if method.access_flags & (ACC_STATIC | ACC_PRIVATE) != 0 {
                continue;
            }
            let Some(params) = params_of(&method.descriptor) else {
                continue;
            };
            let family = (method.name.clone(), params);
            if locked_class || analysis.keep.keeps_member(&method.name) {
                locked_families.insert(family.clone());
            }
            families.entry(family).or_default().push((
                class.this_class.clone(),
                method.name.clone(),
                method.descriptor.clone(),
            ));
        }
    }

    let mut renamed = 0usize;
    let mut family_index = 0u64;
    for (family_key, members) in families {
        if locked_families.contains(&family_key) {
            continue;
        }
        let mut names = NameGenerator::new(
            ctx.rng_nth("rename/virtual-families", family_index),
            cfg.rename.member_style,
        );
        family_index += 1;
        for (_, name, _) in &members {
            names.reserve(name.clone());
        }
        let new_name = names.next_name();
        for key in members {
            if maps.methods.contains_key(&key) {
                continue;
            }
            maps.methods.insert(key, new_name.clone());
            renamed += 1;
        }
    }
    renamed
}

/// `(<params>)<ret>` → `(<params>)`, or `None` if the descriptor is malformed.
fn params_of(descriptor: &str) -> Option<String> {
    let close = descriptor.find(')')?;
    if !descriptor.starts_with('(') {
        return None;
    }
    Some(descriptor[..=close].to_string())
}

pub(crate) fn parent_package(internal: &str) -> String {
    match internal.rfind('/') {
        Some(i) => internal[..i].to_string(),
        None => String::new(),
    }
}

pub(super) fn simple_name(internal: &str) -> String {
    match internal.rfind('/') {
        Some(i) => internal[i + 1..].to_string(),
        None => internal.to_string(),
    }
}

/// Pool of random packages (2-3 segments); a single empty one when spreading
/// is turned off.
pub(super) fn generate_packages(cfg: &Config, rng: &mut Rng) -> Vec<String> {
    if !cfg.rename.spread_packages {
        return vec![String::new()];
    }
    let style = match cfg.rename.class_style {
        NameStyle::Unicode => NameStyle::Unicode,
        NameStyle::IlO0 => NameStyle::IlO0,
        NameStyle::Plausible => NameStyle::Plausible,
        _ => NameStyle::MixedLatin,
    };
    let mut names = NameGenerator::new(rng.clone(), style).for_classes();
    let pool = cfg.rename.package_pool.max(1);
    (0..pool)
        .map(|_| {
            let segments = 2 + rng.below(2) as usize;
            let parts: Vec<String> = (0..segments).map(|_| names.next_name()).collect();
            parts.join("/")
        })
        .collect()
}

/// Rewrite a descriptor/signature: `Lpkg/Class;` → the new class name.
///
/// Generic signatures nest (`LMap<LString;LFoo;>;`): the name scan stops at
/// `<` as well as `;`, so type arguments fall back into the main loop and
/// get rewritten in turn. Stopping only at `;` swallowed the inner
/// reference into one unmapped blob (a stale `Comparable<Old>` leak).
pub(super) fn rewrite_descriptor(desc: &str, classes: &BTreeMap<String, String>) -> String {
    if !desc.contains('L') {
        return desc.to_string();
    }
    let mut out = String::with_capacity(desc.len());
    let mut chars = desc.chars().peekable();
    while let Some(c) = chars.next() {
        if c != 'L' {
            out.push(c);
            continue;
        }
        let mut name = String::new();
        let mut term = ';';
        for c in chars.by_ref() {
            if c == ';' || c == '<' {
                term = c;
                break;
            }
            name.push(c);
        }
        out.push('L');
        out.push_str(classes.get(&name).map_or(name.as_str(), String::as_str));
        out.push(term);
    }
    out
}

/// Widen access to `public` so moving to a different package doesn't break calls.
pub(super) fn widen_all(program: &mut Program) {
    for class in program.classes.iter_mut() {
        class.access_flags = widen(class.access_flags);
        for field in class.fields.iter_mut() {
            field.access_flags = widen(field.access_flags);
        }
        for method in class.methods.iter_mut() {
            method.access_flags = widen(method.access_flags);
        }
    }
}

fn widen(flags: u16) -> u16 {
    (flags & !(ACC_PRIVATE | ACC_PROTECTED)) | ACC_PUBLIC
}
