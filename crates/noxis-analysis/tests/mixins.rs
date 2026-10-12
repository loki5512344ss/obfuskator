//! Mixin keep: classes listed in `*.mixins.json` are now renameable.
//!
//! The resources pass patches mixins.json (rewrites package + entries),
//! so mixin classes can be renamed. Reference: OBF fabric-obfuscator.

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
fn mixin_classes_are_renameable() {
    let program = sample();
    let analysis = Analysis::run(&program, &Config::default()).expect("analysis");

    // Mixin classes are no longer locked - resources pass patches the config.
    for renameable in [
        "demo/mixin/TargetMixin",
        "demo/mixin/other/Helper",
        "demo/mixin/ClientMixin",
        "demo/mixin/Plugin",
    ] {
        assert!(
            !analysis.is_locked(renameable),
            "{renameable} should be renameable now"
        );
    }
}
