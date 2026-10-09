//! Noise pass: junk classes with real bodies, woven into reachability.
//!
//! Dead code gets dropped by shrink analysis, so every junk class gets an
//! edge from live code (an `invokestatic` of its entry). Junk names come
//! from the same generator (and styles) as real code: self-describing
//! `shadow/decoy` names with a uniform `run` entry would be a grep pattern
//! of their own.

mod body;
mod weave;

#[cfg(test)]
mod tests;

use crate::namegen::NameGenerator;
use crate::rename::assign::parent_package;
use body::{entry_body, helper_body};
use noxis_analysis::ClassHierarchy;
use noxis_classfile::{ClassBuilder, Field, Program, Version, compute_frames};
use noxis_core::{Context, CoreError, Pass, PassId, PassReport};
use weave::weave_first_reachable;

/// Flags for the junk entry (`public static`, callable from live code).
const ACC_PUBLIC_STATIC: u16 = 0x0009;
/// Flags for helpers and the accumulator field (`private static`).
const ACC_PRIVATE_STATIC: u16 = 0x000A;

/// The entry descriptor (always `()V`).
const ENTRY_DESCRIPTOR: &str = "()V";
/// Helper methods return the accumulator (`int`).
const HELPER_DESCRIPTOR: &str = "()I";
/// The accumulator field type.
const FIELD_DESCRIPTOR: &str = "I";

/// The noise pass.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoisePass;

impl Pass for NoisePass {
    fn id(&self) -> PassId {
        PassId::Noise
    }

    fn apply(&self, ctx: &mut Context, program: &mut Program) -> Result<PassReport, CoreError> {
        let ratio = ctx.config.noise.junk_ratio;
        let methods_per_class = ctx.config.noise.junk_methods_per_class;
        let annotations = ctx.config.noise.junk_annotations;
        let weave = ctx.config.noise.weave_reachable;

        if ratio == 0 && annotations == 0 {
            return Ok(PassReport {
                pass: PassId::Noise,
                transformed: 0,
                skipped: 0,
                notes: vec!["noise off: junk_ratio=0, junk_annotations=0".to_string()],
            });
        }

        let version = program
            .classes
            .first()
            .map_or(Version::JAVA_21, |c| c.version);
        let real_classes = program.classes.len();
        let count = ratio.saturating_mul(real_classes);

        let mut class_names =
            NameGenerator::new(ctx.rng("noise/classes"), ctx.config.rename.class_style)
                .for_classes();
        for class in &program.classes {
            class_names.reserve(class.this_class.clone());
        }
        let mut rng = ctx.rng("noise");
        // (class, entry) pairs for weaving, in creation order.
        let mut entries: Vec<(String, String)> = Vec::with_capacity(count);
        // (class index, method index) of every generated method, for frames.
        let mut fresh_methods: Vec<(usize, usize)> = Vec::new();
        for index in 0..count {
            let package = if real_classes > 0 {
                let pick = &program.classes[rng.below(real_classes as u64) as usize].this_class;
                parent_package(pick)
            } else {
                String::new()
            };
            let simple = class_names.next_name();
            let name = if package.is_empty() {
                simple
            } else {
                format!("{package}/{simple}")
            };
            let mut member_names = NameGenerator::new(
                ctx.rng_nth("noise/members", index as u64),
                ctx.config.rename.member_style,
            );
            let entry_name = member_names.next_name();
            let field_name = member_names.next_name();
            let mut helpers: Vec<(String, String)> = Vec::new();
            let mut builder = ClassBuilder::new(name.clone(), None).version(version);
            for _ in 0..methods_per_class {
                let helper_name = member_names.next_name();
                let body = helper_body(&mut rng);
                builder = builder.method_with_code(
                    ACC_PRIVATE_STATIC,
                    helper_name.clone(),
                    HELPER_DESCRIPTOR,
                    body,
                );
                helpers.push((name.clone(), helper_name));
            }
            let callees: Vec<(String, String)> = helpers.clone();
            let entry = entry_body(&callees, &name, &field_name);
            builder = builder.method_with_code(
                ACC_PUBLIC_STATIC,
                entry_name.clone(),
                ENTRY_DESCRIPTOR,
                entry,
            );
            let mut junk = builder.build();
            junk.fields.push(Field {
                access_flags: ACC_PRIVATE_STATIC,
                name: field_name,
                descriptor: FIELD_DESCRIPTOR.to_string(),
                attributes: Vec::new(),
            });
            let base = program.classes.len();
            for mi in 0..junk.methods.len() {
                fresh_methods.push((base, mi));
            }
            program.classes.push(junk);
            entries.push((name, entry_name));
        }
        let added = entries.len();

        // Frames for every generated branching method.
        let hierarchy = ClassHierarchy::build(program);
        for (ci, mi) in &fresh_methods {
            refresh_frames(program, *ci, *mi, &hierarchy)?;
        }

        let mut woven = 0usize;
        if weave && added > 0 {
            woven = weave_first_reachable(program, real_classes, &entries)?;
        }

        let mut skipped = 0usize;
        let mut notes = Vec::new();
        if added > 0 {
            notes.push(format!("junk classes added: {added}"));
        }
        if woven > 0 {
            notes.push(format!("woven calls: {woven}"));
        }
        if added == 0 && woven == 0 {
            notes.push("nothing to fuzz: no classes or methods with code".to_string());
        }
        if annotations > 0 {
            skipped = annotations.saturating_mul(program.classes.len());
            notes.push(format!(
                "junk_annotations skipped: raw attributes cannot carry fresh pool indices (skipped: {skipped})"
            ));
        }

        for note in &notes {
            ctx.note(note.clone());
        }

        Ok(PassReport {
            pass: PassId::Noise,
            transformed: added + woven,
            skipped,
            notes,
        })
    }
}

/// Recompute the frames of a generated method.
fn refresh_frames(
    program: &mut Program,
    ci: usize,
    mi: usize,
    hierarchy: &ClassHierarchy,
) -> Result<(), CoreError> {
    let (name, snapshot) = {
        let class = program
            .classes
            .get(ci)
            .ok_or_else(|| CoreError::pass(PassId::Noise, "class vanished"))?;
        (
            class.this_class.clone(),
            class
                .methods
                .get(mi)
                .cloned()
                .ok_or_else(|| CoreError::pass(PassId::Noise, "method vanished"))?,
        )
    };
    let frames = compute_frames(&name, &snapshot, hierarchy)
        .map_err(|e| CoreError::pass(PassId::Noise, e.to_string()))?;
    program
        .classes
        .get_mut(ci)
        .and_then(|c| c.methods.get_mut(mi))
        .and_then(|m| m.code.as_mut())
        .ok_or_else(|| CoreError::pass(PassId::Noise, "method vanished"))?
        .frames = frames;
    Ok(())
}
