//! Mixin keep: classes listed in `*.mixins.json` are locked, not renamed.
//!
//! Real-world proof (LoVisual, 2026-10-13): renaming mixin classes and
//! patching the config breaks the mod at runtime. Two independent failures:
//! 1. When obf packages are randomized per class, the config patcher writes
//!    `package: ""`, and Fabric/Mixin then drops *every* mixin as "orphaned"
//!    (`RenderPipelineMixin implements IRenderPipeline` never applies, so the
//!    injected-interface cast fails with `ClassCastException`).
//! 2. `IMixinConfigPlugin.shouldApplyMixin` compares the runtime mixin class
//!    name against string literals (`Set.of("dev.loki...Mixin")`); renaming
//!    the mixins desyncs those sets and optional mixins get applied when the
//!    target mod is absent.
//!
//! So mixin classes and their plugin stay under their original names, and
//! locking the class freezes its members too - which `@Accessor`/`@Invoker`/
//! `@Overwrite` name lookups require. Other program classes still rename, and
//! calls from locked mixin bytecode are rewritten consistently.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use noxis_analysis::Analysis;
use noxis_classfile::{ClassBuilder, Program, Resource, Version};
use noxis_core::Config;

const MIXINS: &str = r#"{
  "package": "demo.mixin",
  "plugin": "demo.mixin.Plugin",
  "mixins": ["TargetMixin", "other.Helper"],
  "client": ["ClientMixin"]
}"#;

const FABRIC: &str = r#"{
  "id": "demo",
  "mixins": ["demo.mixins.json"]
}"#;

fn sample() -> Program {
    let mut program = Program {
        classes: vec![
            ClassBuilder::new("demo/mixin/TargetMixin", Some("java/lang/Object"))
                .version(Version::JAVA_21)
                .build(),
            ClassBuilder::new("demo/mixin/other/Helper", Some("java/lang/Object"))
                .version(Version::JAVA_21)
                .build(),
            ClassBuilder::new("demo/mixin/ClientMixin", Some("java/lang/Object"))
                .version(Version::JAVA_21)
                .build(),
            ClassBuilder::new("demo/mixin/Plugin", Some("java/lang/Object"))
                .version(Version::JAVA_21)
                .build(),
            ClassBuilder::new("demo/App", Some("java/lang/Object"))
                .version(Version::JAVA_21)
                .build(),
        ],
        resources: Vec::new(),
    };
    program.resources.push(Resource {
        path: "demo.mixins.json".into(),
        data: MIXINS.as_bytes().to_vec(),
    });
    program.resources.push(Resource {
        path: "fabric.mod.json".into(),
        data: FABRIC.as_bytes().to_vec(),
    });
    program
}

#[test]
fn mixin_classes_and_plugin_are_locked() {
    let program = sample();
    let analysis = Analysis::run(&program, &Config::default()).expect("analysis");

    // Mixin entry points and the config plugin are resolved by name at
    // runtime and must keep their names (and, being locked, their members).
    for locked in [
        "demo/mixin/TargetMixin",
        "demo/mixin/other/Helper",
        "demo/mixin/ClientMixin",
        "demo/mixin/Plugin",
    ] {
        assert!(analysis.is_locked(locked), "{locked} should be locked");
    }
}

#[test]
fn non_mixin_classes_stay_renameable() {
    let program = sample();
    let analysis = Analysis::run(&program, &Config::default()).expect("analysis");

    // Plain program classes are not named by any mixin config.
    assert!(!analysis.is_locked("demo/App"));
}
