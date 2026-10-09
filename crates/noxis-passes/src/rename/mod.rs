//! The rename pass: classes, fields, methods, locals, descriptors.
//!
//! The cheapest technique with the highest payoff (see `info/evo-weaknesses.md`:
//! EvoVis never renamed members at all, so the class read plainly via `javap -p`).
//!
//! Subtleties handled here:
//! - a `Sub.staticMethod()` call may name a subclass as the owner - aliases
//!   (`alias`) are needed across the hierarchy;
//! - bridge methods (`ACC_BRIDGE`) and covariant returns live in one method
//!   "family" - they must be renamed together;
//! - `serialVersionUID`, JNI, reflection, serialization stay untouched.

/// Name maps and the assignment strategy.
pub(crate) mod assign;
/// Rewriting method bodies and attributes.
mod rewrite;
/// Pass integration tests.
#[cfg(test)]
mod tests;

use noxis_analysis::{Analysis, ClassHierarchy, RenameSafety, method_rename_safety};
use noxis_classfile::Program;
use noxis_core::{Context, CoreError, Pass, PassId, PassReport, field_key, method_key};

use self::assign::{
    RenameMaps, generate_packages, parent_package, rename_virtual_families, simple_name, widen_all,
};
use self::rewrite::rewrite_class;
use crate::namegen::NameGenerator;

const ACC_PUBLIC: u16 = 0x0001;
const ACC_PRIVATE: u16 = 0x0002;
const ACC_PROTECTED: u16 = 0x0004;
const ACC_STATIC: u16 = 0x0008;
const ACC_FINAL: u16 = 0x0010;
const ACC_ENUM: u16 = 0x4000;

/// The rename pass.
#[derive(Debug, Default, Clone, Copy)]
pub struct RenamePass;

impl Pass for RenamePass {
    fn id(&self) -> PassId {
        PassId::Rename
    }

    fn apply(&self, ctx: &mut Context, program: &mut Program) -> Result<PassReport, CoreError> {
        let analysis = Analysis::run(program, &ctx.config)
            .map_err(|e| CoreError::pass(PassId::Rename, e.to_string()))?;
        let hierarchy = ClassHierarchy::build(program);
        let owners: Vec<String> = program
            .classes
            .iter()
            .map(|c| c.this_class.clone())
            .collect();
        let cfg = ctx.config.clone();

        let mut maps = RenameMaps::default();
        let mut renamed_classes = 0usize;

        // 1. Class names.
        {
            let packages = generate_packages(&cfg, &mut ctx.rng("rename/packages"));
            let mut names =
                NameGenerator::new(ctx.rng("rename/classes"), cfg.rename.class_style).for_classes();
            for name in &owners {
                if analysis.is_locked(name) {
                    names.reserve(simple_name(name));
                }
            }
            for name in &owners {
                if analysis.is_locked(name) {
                    continue;
                }
                let package = if cfg.rename.spread_packages {
                    let idx = ctx
                        .rng_nth("rename/pick", renamed_classes as u64)
                        .below(packages.len() as u64) as usize;
                    packages[idx].clone()
                } else {
                    parent_package(name)
                };
                let simple = names.next_name();
                let new_name = if package.is_empty() {
                    simple
                } else {
                    format!("{package}/{simple}")
                };
                maps.classes.insert(name.clone(), new_name);
                renamed_classes += 1;
            }
        }

        // 2. Simple (non-virtual) members: private/static/final.
        let mut renamed_methods = 0usize;
        let mut renamed_fields = 0usize;

        for (idx, class) in program.classes.iter().enumerate() {
            let owner = class.this_class.clone();
            let locked = analysis.is_locked(&owner);
            let mut method_names = NameGenerator::new(
                ctx.rng_nth("rename/methods", idx as u64),
                cfg.rename.member_style,
            );
            let mut field_names = NameGenerator::new(
                ctx.rng_nth("rename/fields", idx as u64),
                cfg.rename.member_style,
            );

            if !locked {
                for method in &class.methods {
                    if method.name == "<init>" || method.name == "<clinit>" {
                        method_names.reserve(method.name.clone());
                        continue;
                    }
                    if analysis.keep.keeps_member(&method.name) {
                        method_names.reserve(method.name.clone());
                        continue;
                    }
                    let safety = method_rename_safety(
                        program,
                        &hierarchy,
                        &owner,
                        &method.name,
                        &method.descriptor,
                        false,
                    );
                    match safety {
                        RenameSafety::Safe => {
                            let new = method_names.next_name();
                            maps.methods.insert(
                                (
                                    owner.clone(),
                                    method.name.clone(),
                                    method.descriptor.clone(),
                                ),
                                new,
                            );
                            renamed_methods += 1;
                        }
                        // Non-final virtual: renamed only together with the whole
                        // family (see rename_virtual_families).
                        _ => method_names.reserve(method.name.clone()),
                    }
                }

                for field in &class.fields {
                    if analysis.keep.keeps_member(&field.name)
                        || field.name == "serialVersionUID"
                        || field.name == "serialPersistentFields"
                    {
                        field_names.reserve(field.name.clone());
                        continue;
                    }
                    let is_enum_const = field.access_flags & ACC_ENUM != 0;
                    if is_enum_const && !cfg.rename.rename_enum_constants {
                        field_names.reserve(field.name.clone());
                        continue;
                    }
                    let visible = field.access_flags & (ACC_PRIVATE | ACC_STATIC | ACC_FINAL) == 0;
                    if visible && !cfg.rename.rename_public_members {
                        field_names.reserve(field.name.clone());
                        continue;
                    }
                    let new = field_names.next_name();
                    maps.fields.insert(
                        (owner.clone(), field.name.clone(), field.descriptor.clone()),
                        new,
                    );
                    renamed_fields += 1;
                }
            } else {
                for method in &class.methods {
                    method_names.reserve(method.name.clone());
                }
                for field in &class.fields {
                    field_names.reserve(field.name.clone());
                }
            }
        }

        // 3. Virtual families - aggressive mode only.
        if cfg.rename.rename_public_members {
            renamed_methods += rename_virtual_families(program, &analysis, &mut maps, &cfg, ctx);
        }

        // 4. Aliases for subclasses.
        maps.expand(&hierarchy, &owners);

        // 5. Rewrite definitions and all references.
        let mut local_counter = 0u64;
        for class in &mut program.classes {
            let orig_owner = class.this_class.clone();
            rewrite_class(class, &orig_owner, &maps, &cfg, &mut local_counter, ctx);
        }

        // 6. Access widening: moving across packages could otherwise
        //    cause IllegalAccessError.
        if cfg.rename.widen_access {
            widen_all(program);
        }

        // 7. Mappings for the report (stack-trace deobfuscation).
        for ((owner, name, desc), new_name) in maps.methods.iter() {
            ctx.mapping
                .methods
                .insert(method_key(owner, name, desc), new_name.clone());
        }
        for ((owner, name, desc), new_name) in maps.fields.iter() {
            ctx.mapping
                .fields
                .insert(field_key(owner, name, desc), new_name.clone());
        }
        for (old, new_name) in maps.classes.iter() {
            ctx.mapping.classes.insert(old.clone(), new_name.clone());
        }

        let locked = analysis.locked_classes.len();
        if locked > 0 {
            ctx.note(format!("rename: {locked} keep-listed classes (untouched)"));
        }

        Ok(PassReport {
            pass: PassId::Rename,
            transformed: renamed_classes + renamed_methods + renamed_fields,
            skipped: locked,
            notes: vec![format!(
                "classes: {renamed_classes}, methods: {renamed_methods}, fields: {renamed_fields}"
            )],
        })
    }
}
