//! Пасс шума: junk-классы и вплетание их в граф достижимости.
//!
//! Мёртвый код выкидывается shrink-анализом, поэтому каждый junk-класс
//! получает ребро из живого метода (`invokestatic shadow/decoyN.run()V`).
//! Junk-аннотации не генерируются: сырые атрибуты не несут свежие индексы пула.

mod weave;

use noxis_classfile::{ClassBuilder, Program, Version, empty_code};
use noxis_core::{Context, CoreError, Pass, PassId, PassReport};

use weave::weave_first_reachable;

/// Префикс детерминированных имён junk-классов.
const JUNK_PREFIX: &str = "shadow/decoy";

/// Имя и дескриптор точки входа junk-класса.
const JUNK_ENTRY: (&str, &str) = ("run", "()V");

/// Флаги `public static` для junk-метода.
const ACC_PUBLIC_STATIC: u16 = 0x0009;

/// Пасс шума.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoisePass;

impl Pass for NoisePass {
    fn id(&self) -> PassId {
        PassId::Noise
    }

    fn apply(&self, ctx: &mut Context, program: &mut Program) -> Result<PassReport, CoreError> {
        let ratio = ctx.config.noise.junk_ratio;
        let annotations = ctx.config.noise.junk_annotations;
        let weave = ctx.config.noise.weave_reachable;

        if ratio == 0 && annotations == 0 {
            return Ok(PassReport {
                pass: PassId::Noise,
                transformed: 0,
                skipped: 0,
                notes: vec!["шум выключен: junk_ratio=0, junk_annotations=0".to_string()],
            });
        }

        let version = program
            .classes
            .first()
            .map_or(Version::JAVA_21, |c| c.version);
        let real_classes = program.classes.len();
        let count = ratio.saturating_mul(real_classes);

        let mut junk_names = Vec::with_capacity(count);
        let mut suffix = 0usize;
        for _ in 0..count {
            let name = loop {
                let candidate = format!("{JUNK_PREFIX}{suffix}");
                suffix = suffix.saturating_add(1);
                if program.class(&candidate).is_none() {
                    break candidate;
                }
            };
            let junk = ClassBuilder::new(name.clone(), None)
                .version(version)
                .method_with_code(ACC_PUBLIC_STATIC, JUNK_ENTRY.0, JUNK_ENTRY.1, empty_code(0))
                .build();
            program.classes.push(junk);
            junk_names.push(name);
        }
        let added = junk_names.len();

        let mut woven = 0usize;
        if weave && added > 0 {
            woven = weave_first_reachable(program, real_classes, &junk_names)?;
        }

        let mut skipped = 0usize;
        let mut notes = Vec::new();
        if added > 0 {
            notes.push(format!("добавлено junk-классов: {added}"));
        }
        if woven > 0 {
            notes.push(format!("вплетено вызовов: {woven}"));
        }
        if added == 0 && woven == 0 {
            notes.push("нечего шуметь: нет классов или методов с кодом".to_string());
        }
        if annotations > 0 {
            skipped = annotations.saturating_mul(program.classes.len());
            notes.push(format!(
                "junk_annotations пропущен: сырые атрибуты не могут нести свежие индексы пула (пропущено: {skipped})"
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

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use noxis_classfile::{Insn, InvokeKind, Resource};
    use noxis_core::{Config, NoiseConfig};

    use super::*;

    fn sample() -> Program {
        Program {
            classes: vec![
                ClassBuilder::new("demo/App", Some("java/lang/Object"))
                    .version(Version::JAVA_21)
                    .method_with_code(ACC_PUBLIC_STATIC, "run", "()V", empty_code(0))
                    .build(),
            ],
            resources: vec![Resource {
                path: "app.json".into(),
                data: br#"{}"#.to_vec(),
            }],
        }
    }

    fn run(program: &mut Program, noise: NoiseConfig) -> PassReport {
        let config = Config {
            seed: Some(42),
            noise,
            ..Config::default()
        };
        let mut ctx = Context::new(config);
        NoisePass.apply(&mut ctx, program).expect("noise pass")
    }

    #[test]
    fn adds_junk_and_weaves_call() {
        let mut program = sample();
        let noise = NoiseConfig {
            junk_ratio: 2,
            junk_annotations: 0,
            weave_reachable: true,
        };
        let report = run(&mut program, noise);

        assert_eq!(program.classes.len(), 3);
        assert_eq!(program.classes[1].this_class, "shadow/decoy0");
        assert_eq!(program.classes[2].this_class, "shadow/decoy1");

        let insns = &program.classes[0].methods[0].code.as_ref().unwrap().insns;
        assert_eq!(insns.len(), 2);
        match &insns[0] {
            Insn::Invoke {
                kind,
                owner,
                name,
                descriptor,
                itf,
            } => {
                assert_eq!(*kind, InvokeKind::Static);
                assert_eq!(owner, "shadow/decoy0");
                assert_eq!(name, "run");
                assert_eq!(descriptor, "()V");
                assert!(!itf);
            }
            other => panic!("ожидался Invoke, получено: {other:?}"),
        }
        assert!(matches!(insns[1], Insn::Return(None)));
        assert_eq!(report.transformed, 3);
        assert_eq!(report.skipped, 0);
    }

    #[test]
    fn disabled_ratio_changes_nothing() {
        let mut program = sample();
        let before = program.clone();
        let report = run(&mut program, NoiseConfig::default());

        assert_eq!(program, before);
        assert_eq!(report.transformed, 0);
        assert_eq!(report.skipped, 0);
    }
}
