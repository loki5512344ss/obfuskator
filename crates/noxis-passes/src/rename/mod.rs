//! Пасс переименования: классы, поля, методы, локальные переменные, дескрипторы.
//!
//! Это самая дешёвая и самая отдающая техника (см. `info/evo-weaknesses.md`:
//! у EvoVis члены не переименовывались вообще, и класс читался по `javap -p`).
//!
//! Тонкости, которые здесь учитываются:
//! - вызов `Sub.staticMethod()` может указывать владельцем подкласс - нужны
//!   псевдонимы (`alias`) для всей иерархии;
//! - мостовые методы (`ACC_BRIDGE`) и ковариантные возвраты живут в одном
//!   «семействе» метода - переименовывать их нужно разом;
//! - `serialVersionUID`, JNI, рефлексия, сериализация - не трогаем.

/// Карты и стратегия назначения имён.
mod assign;
/// Переписывание тел методов и атрибутов.
mod rewrite;
/// Интеграционные тесты пасса.
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

/// Пасс переименования.
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

        // 1. Имена классов.
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

        // 2. Простые (не-виртуальные) члены: private/static/final.
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
                        // Виртуальный не-final: переименовывается только вместе
                        // со всем семейством (см. rename_virtual_families).
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

        // 3. Виртуальные семейства - только в агрессивном режиме.
        if cfg.rename.rename_public_members {
            renamed_methods += rename_virtual_families(program, &analysis, &mut maps, &cfg, ctx);
        }

        // 4. Псевдоними для наследников.
        maps.expand(&hierarchy, &owners);

        // 5. Переписать определения и все ссылки.
        let mut local_counter = 0u64;
        for class in &mut program.classes {
            let orig_owner = class.this_class.clone();
            rewrite_class(class, &orig_owner, &maps, &cfg, &mut local_counter, ctx);
        }

        // 6. Ширинирование доступа: после переезда в другие пакеты иначе
        //    возможен IllegalAccessError.
        if cfg.rename.widen_access {
            widen_all(program);
        }

        // 7. Маппинги в отчёт (обратная деобфускация stack trace'ей).
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
            ctx.note(format!("rename: {locked} классов в keep (не тронуты)"));
        }

        Ok(PassReport {
            pass: PassId::Rename,
            transformed: renamed_classes + renamed_methods + renamed_fields,
            skipped: locked,
            notes: vec![format!(
                "классов: {renamed_classes}, методов: {renamed_methods}, полей: {renamed_fields}"
            )],
        })
    }
}
