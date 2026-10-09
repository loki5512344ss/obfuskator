//! Пасс отладочной информации: удаление и ложная подмена.
//!
//! Источник требований - `info/evo-weaknesses.md` §3: у разобранного
//! образца оригинальные `SourceFile` у 160 классов, 10 218 `LineNumberTable`
//! и имена локальных переменных. Это дешевле всего чинить и дороже всего
//! отдавать.
//!
//! Важно: удалять недостаточно. Инструменты (JADX/CFR) верят отладке сильнее,
//! чем байткоду, поэтому `spoof_source_file` подставляет **ложное** имя -
//! консистентное, но не связанное с реальным исходником.

use noxis_classfile::{Attribute, ClassFile, Code, Method, Program};
use noxis_core::{Context, CoreError, DebugConfig, Pass, PassId, PassReport};

use crate::namegen::NameGenerator;

/// Атрибут уровня класса/метода, который мы выкидываем как «сырой».
const ENCLOSING_METHOD: &str = "EnclosingMethod";

/// Атрибут уровня метода с именами параметров (`-parameters`).
const METHOD_PARAMETERS: &str = "MethodParameters";

/// Пасс отладочной информации.
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

        let mut stripped = 0usize;
        let mut spoofed = 0usize;

        for class in &mut program.classes {
            spoofed += spoof_source_file(class, &mut source_names, &cfg);
            stripped += strip_signatures(&mut class.attributes, &cfg);

            for method in &mut class.methods {
                stripped += strip_signatures(&mut method.attributes, &cfg);
                stripped += strip_parameters(method, &cfg);
                if let Some(code) = method.code.as_mut() {
                    stripped += strip_code(code, &cfg);
                }
            }

            for field in &mut class.fields {
                stripped += strip_signatures(&mut field.attributes, &cfg);
            }
        }

        let mut notes = Vec::new();
        if stripped > 0 {
            notes.push(format!("удалено атрибутов: {stripped}"));
        }
        if spoofed > 0 {
            notes.push(format!("подменено SourceFile: {spoofed}"));
        }

        Ok(PassReport {
            pass: PassId::DebugInfo,
            transformed: stripped + spoofed,
            skipped: 0,
            notes,
        })
    }
}

/// Оставляет только атрибуты, для которых `keep` вернул `true`.
fn retain(attrs: &mut Vec<Attribute>, keep: impl FnMut(&Attribute) -> bool) -> usize {
    let before = attrs.len();
    attrs.retain(keep);
    before - attrs.len()
}

/// Тела методов: `LineNumberTable`, `LocalVariableTable`.
fn strip_code(code: &mut Code, cfg: &DebugConfig) -> usize {
    retain(&mut code.attributes, |attr| match attr {
        Attribute::LineNumberTable(_) => !cfg.strip_line_numbers,
        Attribute::LocalVariableTable(_) | Attribute::LocalVariableTypeTable(_) => {
            !cfg.strip_local_variables
        }
        _ => true,
    })
}

/// Уровень класса/метода/поля: `Signature`, `InnerClasses`, `EnclosingMethod`.
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

/// Имена параметров метода (`MethodParameters`).
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

/// Подменяет `SourceFile` ложным, но правдоподобным именем.
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

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use noxis_classfile::{ClassBuilder, Insn, LineNumber, LocalVariable, Resource, Version};
    use noxis_core::{Config, Context, RenameConfig};

    use super::*;

    fn sample() -> Program {
        let code = Code {
            max_stack: 0,
            max_locals: 1,
            insns: vec![Insn::Return(None)],
            exception_table: Vec::new(),
            frames: Vec::new(),
            attributes: vec![
                Attribute::LineNumberTable(vec![LineNumber { start: 0, line: 12 }]),
                Attribute::LocalVariableTable(vec![LocalVariable {
                    start: 0,
                    end: 0,
                    name: "this".into(),
                    descriptor: "Ldemo/App;".into(),
                    index: 0,
                }]),
            ],
        };

        let mut class = ClassBuilder::new("demo/App", Some("java/lang/Object"))
            .version(Version::JAVA_21)
            .method_with_code(0x0001, "run", "()V", code)
            .attribute(Attribute::SourceFile("RealApp.java".into()))
            .attribute(Attribute::Signature("Ljava/lang/Object;".into()))
            .attribute(Attribute::InnerClasses(Vec::new()))
            .build();
        class.methods[0].attributes.push(Attribute::Raw {
            name: METHOD_PARAMETERS.into(),
            data: vec![],
        });

        Program {
            classes: vec![class],
            resources: vec![Resource {
                path: "app.json".into(),
                data: br#"{}"#.to_vec(),
            }],
        }
    }

    fn run(program: &mut Program, debug: DebugConfig) -> PassReport {
        let config = Config {
            seed: Some(7),
            debug,
            rename: RenameConfig {
                class_style: noxis_core::NameStyle::ShortLatin,
                ..RenameConfig::default()
            },
            ..Config::default()
        };
        let mut ctx = Context::new(config);
        DebugInfoPass.apply(&mut ctx, program).expect("debug pass")
    }

    #[test]
    fn default_config_strips_and_spoofs() {
        let mut program = sample();
        let report = run(&mut program, DebugConfig::default());

        let class = &program.classes[0];
        let code = class.methods[0].code.as_ref().expect("code");
        assert!(code.attributes.is_empty(), "атрибуты кода должны уйти");
        assert!(
            class.methods[0].attributes.is_empty(),
            "MethodParameters уходит"
        );

        match class.attributes[0] {
            Attribute::SourceFile(ref name) => assert_ne!(name, "RealApp.java"),
            _ => panic!("SourceFile должен быть первым"),
        }
        assert_eq!(class.attributes.len(), 3, "Signature/InnerClasses остаются");
        assert!(report.transformed > 0);
    }

    #[test]
    fn signatures_and_parameters_survive_when_disabled() {
        let debug = DebugConfig {
            strip_line_numbers: false,
            strip_local_variables: false,
            spoof_source_file: false,
            strip_signatures: false,
        };
        let mut program = sample();
        run(&mut program, debug);

        let class = &program.classes[0];
        assert_eq!(class.methods[0].code.as_ref().unwrap().attributes.len(), 2);
        assert_eq!(class.methods[0].attributes.len(), 1);
        assert_eq!(class.attributes.len(), 3);
        assert_eq!(
            class.attributes[0],
            Attribute::SourceFile("RealApp.java".into())
        );
    }

    #[test]
    fn same_seed_gives_the_same_spoofed_name() {
        let mut first = sample();
        let mut second = sample();
        run(&mut first, DebugConfig::default());
        run(&mut second, DebugConfig::default());
        assert_eq!(first.classes[0].attributes, second.classes[0].attributes);
    }
}
