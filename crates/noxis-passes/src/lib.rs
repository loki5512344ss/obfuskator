//! # noxis-passes
//!
//! Bytecode transformations. The pipeline assembly (`standard_pipeline`)
//! lives here as well: the core (`noxis-core`) only knows the [`Pass`] contract.

pub mod constants;
pub mod debug;
pub mod flow;
pub mod namegen;
pub mod noise;
pub mod rename;
pub mod resources;
pub mod strings;

use noxis_core::Pass;

pub use constants::ConstantsPass;
pub use debug::DebugInfoPass;
pub use flow::FlowPass;
pub use namegen::NameGenerator;
pub use noise::NoisePass;
pub use rename::RenamePass;
pub use resources::ResourcesPass;
pub use strings::StringsPass;

/// Standard pipeline in [`docs/architecture.md`](../../docs/architecture.md) order:
/// `rename → resources → strings → constants → flow → noise → debug`.
pub fn standard_pipeline() -> noxis_core::Pipeline {
    noxis_core::Pipeline::new()
        .push(Box::new(RenamePass) as Box<dyn Pass>)
        .push(Box::new(ResourcesPass) as Box<dyn Pass>)
        .push(Box::new(StringsPass) as Box<dyn Pass>)
        .push(Box::new(ConstantsPass) as Box<dyn Pass>)
        .push(Box::new(FlowPass) as Box<dyn Pass>)
        .push(Box::new(NoisePass) as Box<dyn Pass>)
        .push(Box::new(DebugInfoPass) as Box<dyn Pass>)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use noxis_classfile::{ClassBuilder, Code, Insn, Program, Version};
    use noxis_core::{Config, Context, PassId};

    use super::*;

    fn sample() -> Program {
        let code = Code {
            max_stack: 0,
            max_locals: 1,
            insns: vec![Insn::Return(None)],
            exception_table: Vec::new(),
            frames: Vec::new(),
            attributes: Vec::new(),
        };
        Program {
            classes: vec![
                ClassBuilder::new("demo/App", Some("java/lang/Object"))
                    .version(Version::JAVA_21)
                    .method_with_code(0x0001, "run", "()V", code)
                    .build(),
            ],
            resources: Vec::new(),
        }
    }

    #[test]
    fn standard_pipeline_runs_end_to_end_in_order() {
        let mut ctx = Context::new(Config {
            seed: Some(1),
            enable: vec![PassId::Flow],
            ..Config::default()
        });
        let mut program = sample();

        let report = standard_pipeline()
            .run(&mut ctx, &mut program)
            .expect("pipeline");

        let order: Vec<_> = report.passes.iter().map(|p| p.pass).collect();
        assert_eq!(
            order,
            vec![
                PassId::Rename,
                PassId::Resources,
                PassId::Strings,
                PassId::Constants,
                PassId::Flow,
                PassId::Noise,
                PassId::DebugInfo
            ]
        );
        assert_eq!(report.counters.get("classes"), Some(&1));
    }
}
