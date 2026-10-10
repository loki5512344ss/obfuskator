//! Mixin keep: classes listed in `*.mixins.json` (and the plugin) stay.
//!
//! The Mixin framework resolves mixin classes and their members by name at
//! runtime, so renaming them breaks the mod at startup. Entries use simple
//! names relative to the `package` field, which generic text patching can
//! never see - locking is the only sound answer.

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
fn mixin_classes_and_plugin_stay() {
    let program = sample();
    let analysis = Analysis::run(&program, &Config::default()).expect("analysis");

    for kept in [
        "demo/mixin/TargetMixin",
        "demo/mixin/other/Helper",
        "demo/mixin/ClientMixin",
        "demo/mixin/Plugin",
    ] {
        assert!(analysis.is_locked(kept), "{kept} must survive renaming");
    }
    assert!(
        !analysis.is_locked("demo/App"),
        "ordinary classes must still rename"
    );
}
