//! Virtual-family integrity: a package-private override chain must be
//! renamed as one family, never member by member.
//!
//! `method_rename_safety` used to classify package-private overrides as
//! `Safe` (its subclass check never fired: `declaring_chain` only walks
//! upwards). The individual renames then split one family into different
//! names, and the JVM failed at dispatch with `AbstractMethodError`
//! (real case: shaded gson `ReflectionHelper$RecordHelper.isRecord`).

#![allow(clippy::expect_used, clippy::unwrap_used)]

use noxis_classfile::{ClassBuilder, Code, Const, Insn, Method, Prim, Program, Version};
use noxis_core::{Config, Context, Pass, RenameConfig};
use noxis_passes::RenamePass;

fn check_code() -> Code {
    Code {
        max_stack: 1,
        max_locals: 2,
        insns: vec![Insn::Const(Const::Int(1)), Insn::Return(Some(Prim::Int))],
        exception_table: Vec::new(),
        frames: Vec::new(),
        attributes: Vec::new(),
    }
}

/// `demo/Base` declares package-private abstract `check(LClass;)Z`;
/// `demo/ImplA`/`demo/ImplB` override it.
fn sample() -> Program {
    let mut base = ClassBuilder::new("demo/Base", Some("java/lang/Object"))
        .version(Version::JAVA_21)
        .build();
    base.methods.push(Method {
        access_flags: 0x0400, // abstract, package-private
        name: "check".to_string(),
        descriptor: "(Ljava/lang/Class;)Z".to_string(),
        code: None,
        attributes: Vec::new(),
    });
    let impl_a = ClassBuilder::new("demo/ImplA", Some("demo/Base"))
        .version(Version::JAVA_21)
        .method_with_code(0x0000, "check", "(Ljava/lang/Class;)Z", check_code())
        .build();
    let impl_b = ClassBuilder::new("demo/ImplB", Some("demo/Base"))
        .version(Version::JAVA_21)
        .method_with_code(0x0000, "check", "(Ljava/lang/Class;)Z", check_code())
        .build();
    Program {
        classes: vec![base, impl_a, impl_b],
        resources: Vec::new(),
    }
}

fn run(program: &mut Program, rename_public_members: bool) {
    let config = Config {
        seed: Some(7),
        rename: RenameConfig {
            rename_public_members,
            ..RenameConfig::default()
        },
        ..Config::default()
    };
    let mut ctx = Context::new(config);
    RenamePass.apply(&mut ctx, program).expect("rename pass");
}

/// The three `check(LClass;)Z` methods after the pass: one shared name.
fn family_names(program: &Program) -> Vec<String> {
    let mut out = Vec::new();
    for class in &program.classes {
        for method in &class.methods {
            if method.descriptor == "(Ljava/lang/Class;)Z" {
                out.push(method.name.clone());
            }
        }
    }
    out
}

#[test]
fn package_private_override_chain_renames_as_one_family() {
    let mut program = sample();
    run(&mut program, true);

    let names = family_names(&program);
    assert_eq!(names.len(), 3, "all three overrides must be present");
    assert!(
        names.iter().all(|name| name == &names[0]),
        "one family - one name, got {names:?}"
    );
    assert_ne!(names[0], "check", "the family must actually be renamed");
}

#[test]
fn package_private_overrides_stay_consistent_without_public_rename() {
    let mut program = sample();
    run(&mut program, false);

    let names = family_names(&program);
    assert_eq!(names.len(), 3, "all three overrides must be present");
    assert!(
        names.iter().all(|name| name == &names[0]),
        "families off must keep the chain untouched, got {names:?}"
    );
}
