//! The strings pass: `Const::String` becomes runtime-decoder calls.
//!
//! The scheme rotates per string (`salt % n`), the salt is deterministic
//! (KDF or RNG stream - see `derive_key`). The decoder is generated once per
//! program and only if at least one string was encrypted.

mod condy;
mod crypt;

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use noxis_analysis::{ClassHierarchy, JdkHierarchy};
use noxis_classfile::{
    Attribute, BootstrapMethod, Const, Insn, MAX_METHOD_INSNS, Program, compute_frames,
};
use noxis_core::{Context, CoreError, NameStyle, Pass, PassId, PassReport, derive_seed};
use noxis_runtime::{Scheme, decoder_class, decoder_owner, encrypt};

use crate::namegen::NameGenerator;
use crypt::{decode_sequence, salt_for_scheme, split_chunks};

/// The string encryption pass.
#[derive(Debug, Default, Clone, Copy)]
pub struct StringsPass;

impl Pass for StringsPass {
    fn id(&self) -> PassId {
        PassId::Strings
    }

    fn apply(&self, ctx: &mut Context, program: &mut Program) -> Result<PassReport, CoreError> {
        let cfg = ctx.config.strings.clone();
        let mut schemes = Vec::with_capacity(cfg.schemes.len());
        for name in &cfg.schemes {
            schemes.push(Scheme::parse(name).map_err(|e| pass_error(e.to_string()))?);
        }
        if schemes.is_empty() {
            return Err(pass_error("no encryption schemes"));
        }
        // Phase 1: collect every encryptable string (position + text).
        let mut targets: Vec<(usize, usize, usize, String)> = Vec::new();
        let mut skipped = 0usize;
        for (ci, class) in program.classes.iter().enumerate() {
            for (mi, method) in class.methods.iter().enumerate() {
                let Some(code) = method.code.as_ref() else {
                    continue;
                };
                for (ii, insn) in code.insns.iter().enumerate() {
                    if let Insn::Const(Const::String(text)) = insn {
                        if text.chars().count() >= cfg.min_len && !is_skipped(text, &cfg.skip) {
                            targets.push((ci, mi, ii, text.clone()));
                        } else {
                            skipped += 1;
                        }
                    }
                }
            }
        }
        if targets.is_empty() {
            return Ok(report(
                0,
                skipped,
                vec!["no strings to encrypt".to_string()],
            ));
        }
        // Phase 2: encrypt and substitute call sequences.
        let master = ctx.master_seed;
        let mut rng = ctx.rng("strings");
        let owner = decoder_name(ctx, program);
        let mut chunk_counter = 0usize;
        let mut salt_counter = 0u64;
        // Condy mode: per-class next bootstrap index + accumulated entries
        // (appended to the class attribute after the rewrite below).
        let mut bsm_next: BTreeMap<usize, u16> = BTreeMap::new();
        let mut bsm_entries: BTreeMap<usize, Vec<BootstrapMethod>> = BTreeMap::new();
        if cfg.condy {
            for (ci, class) in program.classes.iter().enumerate() {
                if condy::supported(class) {
                    bsm_next.insert(ci, condy::base_index(class));
                }
            }
        }
        let mut replacements: BTreeMap<(usize, usize, usize), Vec<Insn>> = BTreeMap::new();
        for (ci, mi, ii, text) in &targets {
            let mut parts = Vec::new();
            for chunk in split_chunks(text, cfg.split_above) {
                let scheme = chunk_counter % schemes.len();
                let raw = next_salt(&mut rng, master, &mut salt_counter, cfg.derive_key);
                let salt = salt_for_scheme(raw, scheme, schemes.len());
                let blob = encrypt(schemes[scheme], master, salt, &chunk);
                parts.push((blob.cipher, salt));
                chunk_counter += 1;
            }
            if let Some(next) = bsm_next.get_mut(ci) {
                let base = *next;
                for (cipher, salt) in &parts {
                    bsm_entries
                        .entry(*ci)
                        .or_default()
                        .push(condy::entry(&owner, cipher, *salt));
                }
                *next = base + parts.len() as u16;
                replacements.insert((*ci, *mi, *ii), condy::sequence(base, parts.len()));
            } else {
                replacements.insert((*ci, *mi, *ii), decode_sequence(&owner, &parts));
            }
        }
        let mut mutated: Vec<(usize, usize)> = Vec::new();
        let mut applied = 0usize;
        let mut per_method: BTreeMap<(usize, usize), usize> = BTreeMap::new();
        for (ci, mi, _, _) in &targets {
            *per_method.entry((*ci, *mi)).or_default() += 1;
        }
        for (ci, class) in program.classes.iter_mut().enumerate() {
            for (mi, method) in class.methods.iter_mut().enumerate() {
                let Some(code) = method.code.as_mut() else {
                    continue;
                };
                let original = std::mem::take(&mut code.insns);
                let method_targets = per_method.get(&(ci, mi)).copied().unwrap_or(0);
                let mut fresh = Vec::with_capacity(original.len());
                let mut touched = false;
                for (ii, insn) in original.iter().enumerate() {
                    if let Some(sequence) = replacements.remove(&(ci, mi, ii)) {
                        fresh.extend(sequence);
                        touched = true;
                    } else {
                        fresh.push(insn.clone());
                    }
                }
                if fresh.len() > MAX_METHOD_INSNS {
                    // Would risk the 64KB writer ceiling: keep the original
                    // code (and its strings) untouched.
                    code.insns = original;
                    skipped += 1;
                    continue;
                }
                code.insns = fresh;
                if touched {
                    applied += method_targets;
                    code.max_stack = code.max_stack.saturating_add(2);
                    mutated.push((ci, mi));
                }
            }
        }
        // Phase 3: one decoder class per program.
        let scheme_names: Vec<&str> = cfg.schemes.iter().map(String::as_str).collect();
        let decoder =
            decoder_class(&owner, master, &scheme_names).map_err(|e| pass_error(e.to_string()))?;
        program.classes.push(decoder);
        ctx.note(format!("string decoder: {owner}"));
        // Phase 3b: condy bootstrap entries land on their classes (appended
        // after existing lambdas - the indices computed above assumed that).
        for (ci, entries) in bsm_entries {
            let class = program
                .classes
                .get_mut(ci)
                .ok_or_else(|| pass_error("class vanished"))?;
            match class.attributes.iter_mut().find_map(|a| match a {
                Attribute::BootstrapMethods(list) => Some(list),
                _ => None,
            }) {
                Some(list) => list.extend(entries),
                None => class.attributes.push(Attribute::BootstrapMethods(entries)),
            }
        }
        // Phase 4: frames of the touched methods.
        let hierarchy = ClassHierarchy::build(program);
        let frames_hierarchy = JdkHierarchy::wrap(&hierarchy);
        for (ci, mi) in &mutated {
            let (name, snapshot) = {
                let class = program
                    .classes
                    .get(*ci)
                    .ok_or_else(|| pass_error("class vanished"))?;
                (
                    class.this_class.clone(),
                    class
                        .methods
                        .get(*mi)
                        .cloned()
                        .ok_or_else(|| pass_error("method vanished"))?,
                )
            };
            let frames = compute_frames(&name, &snapshot, &frames_hierarchy)
                .map_err(|e| pass_error(e.to_string()))?;
            program
                .classes
                .get_mut(*ci)
                .and_then(|c| c.methods.get_mut(*mi))
                .and_then(|m| m.code.as_mut())
                .ok_or_else(|| pass_error("method vanished"))?
                .frames = frames;
        }
        Ok(report(
            applied,
            skipped,
            vec![format!("strings encrypted: {applied}, decoder: {owner}")],
        ))
    }
}

fn report(transformed: usize, skipped: usize, notes: Vec<String>) -> PassReport {
    PassReport {
        pass: PassId::Strings,
        transformed,
        skipped,
        notes,
    }
}

fn pass_error(msg: impl Into<String>) -> CoreError {
    CoreError::pass(PassId::Strings, msg)
}

/// A string matches the `skip` patterns of the config.
fn is_skipped(text: &str, skip: &[String]) -> bool {
    skip.iter().any(|pattern| text.contains(pattern))
}

/// A chunk salt: KDF from the position or an RNG stream (see `derive_key`).
fn next_salt(rng: &mut noxis_core::Rng, master: u64, counter: &mut u64, derive: bool) -> u64 {
    let value = if derive {
        derive_seed(master, "strings/salt", *counter)
    } else {
        rng.next_u64()
    };
    *counter += 1;
    value
}

/// A collision-free deterministic decoder name.
fn decoder_name(ctx: &mut Context, program: &Program) -> String {
    let mut names = NameGenerator::new(ctx.rng("strings/decoder"), NameStyle::ShortLatin);
    for class in &program.classes {
        names.reserve(class.this_class.clone());
    }
    loop {
        let owner = decoder_owner(&format!("{}/{}", names.next_name(), names.next_name()));
        if program.class(&owner).is_none() {
            return owner;
        }
    }
}
