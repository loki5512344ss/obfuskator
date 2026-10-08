//! Пасс шифрования строк: `Const::String` → вызовы рантайм-декодера.
//!
//! Схема строки выбирается ротацией (`salt % n`), salt детерминирован
//! (KDF или RNG-поток — см. `derive_key`). Декодер генерируется один на
//! программу и только если была зашифрована хотя бы одна строка.

mod crypt;

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use noxis_analysis::ClassHierarchy;
use noxis_classfile::{Const, Insn, Program, compute_frames};
use noxis_core::{Context, CoreError, NameStyle, Pass, PassId, PassReport, derive_seed};
use noxis_runtime::{Scheme, decoder_class, decoder_owner, encrypt};

use crate::namegen::NameGenerator;
use crypt::{decode_sequence, salt_for_scheme, split_chunks};

/// Пасс шифрования строк.
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
            return Err(pass_error("нет схем шифрования"));
        }
        // Фаза 1: собрать все шифруемые строки (позиция + текст).
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
                vec!["нет строк для шифрования".to_string()],
            ));
        }
        // Фаза 2: зашифровать и подставить последовательности вызовов.
        let master = ctx.master_seed;
        let mut rng = ctx.rng("strings");
        let owner = decoder_name(ctx, program);
        let mut chunk_counter = 0usize;
        let mut salt_counter = 0u64;
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
            replacements.insert((*ci, *mi, *ii), decode_sequence(&owner, &parts));
        }
        let mut mutated: Vec<(usize, usize)> = Vec::new();
        for (ci, class) in program.classes.iter_mut().enumerate() {
            for (mi, method) in class.methods.iter_mut().enumerate() {
                let Some(code) = method.code.as_mut() else {
                    continue;
                };
                let mut fresh = Vec::with_capacity(code.insns.len());
                let mut touched = false;
                for (ii, insn) in code.insns.drain(..).enumerate() {
                    if let Some(sequence) = replacements.remove(&(ci, mi, ii)) {
                        fresh.extend(sequence);
                        touched = true;
                    } else {
                        fresh.push(insn);
                    }
                }
                if touched {
                    code.max_stack = code.max_stack.saturating_add(2);
                    mutated.push((ci, mi));
                }
                code.insns = fresh;
            }
        }
        // Фаза 3: класс-декодер один на программу.
        let scheme_names: Vec<&str> = cfg.schemes.iter().map(String::as_str).collect();
        let decoder =
            decoder_class(&owner, master, &scheme_names).map_err(|e| pass_error(e.to_string()))?;
        program.classes.push(decoder);
        ctx.note(format!("декодер строк: {owner}"));
        // Фаза 4: фреймы затронутых методов.
        let hierarchy = ClassHierarchy::build(program);
        for (ci, mi) in &mutated {
            let (name, snapshot) = {
                let class = program
                    .classes
                    .get(*ci)
                    .ok_or_else(|| pass_error("класс пропал"))?;
                (
                    class.this_class.clone(),
                    class
                        .methods
                        .get(*mi)
                        .cloned()
                        .ok_or_else(|| pass_error("метод пропал"))?,
                )
            };
            let frames = compute_frames(&name, &snapshot, &hierarchy)
                .map_err(|e| pass_error(e.to_string()))?;
            program
                .classes
                .get_mut(*ci)
                .and_then(|c| c.methods.get_mut(*mi))
                .and_then(|m| m.code.as_mut())
                .ok_or_else(|| pass_error("метод пропал"))?
                .frames = frames;
        }
        Ok(report(
            targets.len(),
            skipped,
            vec![format!(
                "зашифровано строк: {}, декодер: {owner}",
                targets.len()
            )],
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

/// Строка подпадает под `skip`-паттерны конфига.
fn is_skipped(text: &str, skip: &[String]) -> bool {
    skip.iter().any(|pattern| text.contains(pattern))
}

/// Salt чанка: KDF от позиции или RNG-поток (см. `derive_key`).
fn next_salt(rng: &mut noxis_core::Rng, master: u64, counter: &mut u64, derive: bool) -> u64 {
    let value = if derive {
        derive_seed(master, "strings/salt", *counter)
    } else {
        rng.next_u64()
    };
    *counter += 1;
    value
}

/// Детерминированное имя декодера без коллизий с классами программы.
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
