//! Debug-info pass tests: strip, spoof, fake lines, source removal.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use noxis_classfile::{
    Attribute, ClassBuilder, Code, Insn, LineNumber, LocalVariable, Program, Resource, Version,
};
use noxis_core::{Config, Context, DebugConfig, Pass, PassReport, RenameConfig};
use noxis_passes::DebugInfoPass;

fn sample() -> Program {
    let code = Code {
        max_stack: 0,
        max_locals: 1,
        insns: vec![Insn::Return(None)],
        exception_table: Vec::new(),
        frames: Vec::new(),
        attributes: vec![
            Attribute::LineNumberTable(vec![
                LineNumber { start: 0, line: 12 },
                LineNumber { start: 0, line: 15 },
            ]),
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
        name: "MethodParameters".into(),
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
    assert!(code.attributes.is_empty(), "code attributes must go");
    assert!(
        class.methods[0].attributes.is_empty(),
        "MethodParameters goes"
    );

    match class.attributes[0] {
        Attribute::SourceFile(ref name) => assert_ne!(name, "RealApp.java"),
        _ => panic!("SourceFile must come first"),
    }
    assert_eq!(class.attributes.len(), 3, "Signature/InnerClasses stay");
    assert!(report.transformed > 0);
}

#[test]
fn signatures_and_parameters_survive_when_disabled() {
    let debug = DebugConfig {
        strip_line_numbers: false,
        fake_line_numbers: false,
        strip_local_variables: false,
        spoof_source_file: false,
        strip_source_file: false,
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

fn line_numbers(program: &Program) -> Vec<u16> {
    let code = program.classes[0].methods[0].code.as_ref().expect("code");
    code.attributes
        .iter()
        .find_map(|attr| match attr {
            Attribute::LineNumberTable(rows) => Some(rows.iter().map(|r| r.line).collect()),
            _ => None,
        })
        .unwrap_or_default()
}

/// Fake lines replace the table instead of removing it, deterministically.
#[test]
fn fake_line_numbers_replace_not_remove() {
    let debug = DebugConfig {
        strip_line_numbers: false,
        fake_line_numbers: true,
        ..DebugConfig::default()
    };
    let mut first = sample();
    let mut second = sample();
    run(&mut first, debug.clone());
    run(&mut second, debug);

    for program in [&first, &second] {
        let lines = line_numbers(program);
        assert_eq!(lines.len(), 2, "table must survive with 2 rows");
        assert_ne!(lines, vec![12, 15], "numbers must change");
        assert!(lines.iter().all(|l| *l > 0), "numbers stay plausible");
    }
    assert_eq!(
        line_numbers(&first),
        line_numbers(&second),
        "same seed, same fake lines"
    );
}

/// Stripping the source removes the attribute even with spoofing on.
#[test]
fn strip_source_file_removes() {
    let debug = DebugConfig {
        strip_source_file: true,
        ..DebugConfig::default()
    };
    let mut program = sample();
    run(&mut program, debug);

    assert!(
        program.classes[0]
            .attributes
            .iter()
            .all(|attr| !matches!(attr, Attribute::SourceFile(_))),
        "no SourceFile may survive"
    );
}

/// Stripping wins over faking: a removed table has nothing to fake.
#[test]
fn strip_wins_over_fake() {
    let debug = DebugConfig {
        strip_line_numbers: true,
        fake_line_numbers: true,
        ..DebugConfig::default()
    };
    let mut program = sample();
    run(&mut program, debug);

    assert!(line_numbers(&program).is_empty());
}
