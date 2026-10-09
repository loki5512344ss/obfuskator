//! The debug-info pass: removal and false replacement.
//!
//! Requirements come from `info/evo-weaknesses.md` §3: the dissected sample
//! kept original `SourceFile`s on 160 classes, 10 218 `LineNumberTable`s and
//! local variable names. This is the cheapest thing to fix and the most
//! expensive to give away.
//!
//! Removal alone is not enough. Tools (JADX/CFR) trust debug info more than
//! bytecode, so `spoof_source_file` substitutes a **false** name -
//! consistent, but unrelated to the real sources. `fake_line_numbers` does
//! the same to line numbers (a plausible walk instead of the truth), and
//! `strip_source_file` drops the attribute entirely.

use noxis_classfile::{Attribute, ClassFile, Code, Method, Program};
use noxis_core::{Context, CoreError, DebugConfig, Pass, PassId, PassReport, Rng};

use crate::namegen::NameGenerator;

/// A class/method-level attribute dropped as "raw".
const ENCLOSING_METHOD: &str = "EnclosingMethod";

/// A method-level attribute holding parameter names (`-parameters`).
const METHOD_PARAMETERS: &str = "MethodParameters";

/// The debug-info pass.
#[derive(Debug, Default, Clone, Copy)]
pub struct DebugInfoPass;

impl Pass for DebugInfoPass {
    fn id(&self) -> PassId {
        PassId::DebugInfo
    }

    fn apply(&self, ctx: &mut Context, program: &mut Program) -> Result<PassReport, CoreError> {
        let cfg = ctx.config.debug.clone();
        let style = ctx.config.rename.class_style;
        let mut source_names =
            NameGenerator::new(ctx.rng("debug/source_file"), style).for_classes();
        let mut line_rng = ctx.rng("debug/lines");

        let mut stripped = 0usize;
        let mut spoofed = 0usize;
        let mut faked = 0usize;

        for class in &mut program.classes {
            stripped += strip_source_file(class, &cfg);
            spoofed += spoof_source_file(class, &mut source_names, &cfg);
            stripped += strip_signatures(&mut class.attributes, &cfg);

            for method in &mut class.methods {
                stripped += strip_signatures(&mut method.attributes, &cfg);
                stripped += strip_parameters(method, &cfg);
                if let Some(code) = method.code.as_mut() {
                    stripped += strip_code(code, &cfg);
                    faked += fake_line_numbers(code, &mut line_rng, &cfg);
                }
            }

            for field in &mut class.fields {
                stripped += strip_signatures(&mut field.attributes, &cfg);
            }
        }

        let mut notes = Vec::new();
        if stripped > 0 {
            notes.push(format!("attributes removed: {stripped}"));
        }
        if spoofed > 0 {
            notes.push(format!("SourceFile spoofed: {spoofed}"));
        }
        if faked > 0 {
            notes.push(format!("line numbers faked: {faked}"));
        }

        Ok(PassReport {
            pass: PassId::DebugInfo,
            transformed: stripped + spoofed + faked,
            skipped: 0,
            notes,
        })
    }
}

/// Keep only the attributes for which `keep` returns true.
fn retain(attrs: &mut Vec<Attribute>, keep: impl FnMut(&Attribute) -> bool) -> usize {
    let before = attrs.len();
    attrs.retain(keep);
    before - attrs.len()
}

/// Method bodies: `LineNumberTable`, `LocalVariableTable`.
fn strip_code(code: &mut Code, cfg: &DebugConfig) -> usize {
    retain(&mut code.attributes, |attr| match attr {
        Attribute::LineNumberTable(_) => !cfg.strip_line_numbers,
        Attribute::LocalVariableTable(_) | Attribute::LocalVariableTypeTable(_) => {
            !cfg.strip_local_variables
        }
        _ => true,
    })
}

/// Class/method/field level: `Signature`, `InnerClasses`, `EnclosingMethod`.
fn strip_signatures(attrs: &mut Vec<Attribute>, cfg: &DebugConfig) -> usize {
    if !cfg.strip_signatures {
        return 0;
    }
    retain(attrs, |attr| match attr {
        Attribute::Signature(_) | Attribute::InnerClasses(_) => false,
        Attribute::EnclosingMethod { .. } => false,
        Attribute::Raw { name, .. } => name != ENCLOSING_METHOD,
        _ => true,
    })
}

/// Method parameter names (`MethodParameters`).
fn strip_parameters(method: &mut Method, cfg: &DebugConfig) -> usize {
    if !cfg.strip_local_variables {
        return 0;
    }
    retain(&mut method.attributes, |attr| match attr {
        Attribute::MethodParameters(_) => false,
        Attribute::Raw { name, .. } => name != METHOD_PARAMETERS,
        _ => true,
    })
}

/// Replace `LineNumberTable` rows with plausible-but-wrong numbers.
///
/// Runs only when stripping is off (a removed table has nothing to fake):
/// a deterministic upward walk, so stack traces still look like traces
/// instead of pointing at the real lines.
fn fake_line_numbers(code: &mut Code, rng: &mut Rng, cfg: &DebugConfig) -> usize {
    if !cfg.fake_line_numbers || cfg.strip_line_numbers {
        return 0;
    }
    let mut faked = 0usize;
    let mut line = 1 + rng.below(200) as u16;
    for attr in &mut code.attributes {
        if let Attribute::LineNumberTable(rows) = attr {
            for row in rows.iter_mut() {
                row.line = line;
                line = line.saturating_add(1 + rng.below(8) as u16);
                faked += 1;
            }
        }
    }
    faked
}

/// Remove `SourceFile` entirely instead of spoofing it.
fn strip_source_file(class: &mut ClassFile, cfg: &DebugConfig) -> usize {
    if !cfg.strip_source_file {
        return 0;
    }
    retain(&mut class.attributes, |attr| {
        !matches!(attr, Attribute::SourceFile(_))
    })
}

/// Replace `SourceFile` with a false but plausible name.
fn spoof_source_file(class: &mut ClassFile, names: &mut NameGenerator, cfg: &DebugConfig) -> usize {
    if !cfg.spoof_source_file {
        return 0;
    }
    let mut replaced = 0;
    for attr in &mut class.attributes {
        if matches!(attr, Attribute::SourceFile(_)) {
            *attr = Attribute::SourceFile(format!("{}.java", names.next_name()));
            replaced += 1;
        }
    }
    replaced
}
