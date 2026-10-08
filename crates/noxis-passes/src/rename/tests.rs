//! Тесты пасса переименования: keep, ссылки, семейства, алиасы.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeMap;

use noxis_analysis::ClassHierarchy;
use noxis_classfile::{
    ClassBuilder, ExceptionHandler, Insn, InvokeKind, Program, Resource, empty_code,
};
use noxis_core::{Config, Context, Pass, RenameConfig};

use super::RenamePass;
use super::assign::{RenameMaps, rewrite_descriptor};

fn sample() -> Program {
    let main = ClassBuilder::new("app/Main", None)
        .method_with_code(
            0x0009,
            "main",
            "([Ljava/lang/String;)V",
            noxis_classfile::Code {
                max_stack: 4,
                max_locals: 1,
                insns: vec![
                    Insn::Invoke {
                        kind: InvokeKind::Static,
                        owner: "app/Helper".into(),
                        name: "make".into(),
                        descriptor: "()Lapp/Helper;".into(),
                        itf: false,
                    },
                    Insn::Pop,
                    Insn::Return(None),
                ],
                exception_table: Vec::new(),
                frames: Vec::new(),
                attributes: Vec::new(),
            },
        )
        .build();
    let helper = ClassBuilder::new("app/Helper", None)
        .method_with_code(
            0x0009 | 0x0001,
            "make",
            "()Lapp/Helper;",
            noxis_classfile::Code {
                max_stack: 1,
                max_locals: 0,
                insns: vec![Insn::New("app/Helper".into()), Insn::Return(None)],
                exception_table: Vec::new(),
                frames: Vec::new(),
                attributes: Vec::new(),
            },
        )
        .method_with_code(0x0002, "secret", "()V", empty_code(0))
        .build();
    Program {
        classes: vec![main, helper],
        resources: vec![Resource {
            path: "data.txt".into(),
            data: b"x".to_vec(),
        }],
    }
}

fn run_rename(program: &mut Program) -> Context {
    let config = Config {
        seed: Some(9),
        keep: vec!["app/Main".to_string()],
        rename: RenameConfig {
            package_pool: 4,
            ..RenameConfig::default()
        },
        ..Config::default()
    };
    let mut ctx = Context::new(config);
    let report = RenamePass.apply(&mut ctx, program).expect("rename");
    assert!(report.transformed > 0, "ничего не переименовано");
    ctx
}

#[test]
fn renames_unlocked_class_and_references() {
    let mut program = sample();
    let ctx = run_rename(&mut program);

    assert_eq!(
        program.class("app/Main").map(|c| c.this_class.as_str()),
        Some("app/Main"),
        "keep-класс обязан остаться"
    );
    let helper = program
        .classes
        .iter()
        .find(|c| c.this_class != "app/Main")
        .cloned()
        .expect("helper");
    assert_ne!(helper.this_class, "app/Helper");

    let main = program.class("app/Main").expect("main");
    let code = main.methods[0].code.as_ref().expect("code");
    let (new_owner, new_desc) = match &code.insns[0] {
        Insn::Invoke {
            owner, descriptor, ..
        } => (owner.clone(), descriptor.clone()),
        other => panic!("ожидался Invoke, получено {other:?}"),
    };
    assert_eq!(new_owner, helper.this_class);
    assert!(new_desc.contains(&helper.this_class), "desc: {new_desc}");

    let renamed_secret = helper
        .methods
        .iter()
        .find(|m| m.name != "make")
        .map(|m| m.name.clone())
        .expect("secret переименован");
    assert_ne!(renamed_secret, "secret");
    assert!(!ctx.mapping.methods.is_empty());
}

#[test]
fn descriptor_rewrite_handles_arrays() {
    let mut map = BTreeMap::new();
    map.insert("a/b/C".to_string(), "p/Q".to_string());
    map.insert("a/b/D".to_string(), "p/R".to_string());
    assert_eq!(
        rewrite_descriptor("(La/b/C;[La/b/D;)La/b/C;", &map),
        "(Lp/Q;[Lp/R;)Lp/Q;"
    );
    assert_eq!(rewrite_descriptor("([[I)V", &map), "([[I)V");
}

#[test]
fn exception_handler_type_is_rewritten() {
    let mut program = sample();
    program
        .classes
        .push(ClassBuilder::new("app/Boom", None).build());
    program.classes[1].methods[0]
        .code
        .as_mut()
        .expect("code")
        .exception_table
        .push(ExceptionHandler {
            start: 0,
            end: 1,
            handler: 2,
            catch_type: Some("app/Boom".into()),
        });
    let ctx = run_rename(&mut program);
    let new_boom = ctx.mapping.class("app/Boom").expect("boom renamed");
    let helper = program
        .classes
        .iter()
        .find(|c| c.this_class != "app/Main")
        .expect("helper");
    let handler = &helper.methods[0]
        .code
        .as_ref()
        .expect("code")
        .exception_table[0];
    assert_eq!(handler.catch_type.as_deref(), Some(new_boom));
}

#[test]
fn subclass_gets_alias_for_inherited_member() {
    let base = ClassBuilder::new("p/Base", None)
        .method_with_code(0x0009, "shared", "()V", empty_code(0))
        .build();
    let sub = ClassBuilder::new("p/Sub", Some("p/Base")).build();
    let program = Program {
        classes: vec![base, sub],
        resources: Vec::new(),
    };
    let hierarchy = ClassHierarchy::build(&program);
    let mut maps = RenameMaps::default();
    maps.methods.insert(
        ("p/Base".into(), "shared".into(), "()V".into()),
        "zz".into(),
    );
    let owners = vec!["p/Base".to_string(), "p/Sub".to_string()];
    maps.expand(&hierarchy, &owners);

    // Вызов через подкласс обязан находить новое имя.
    assert_eq!(
        maps.method("p/Base", "shared", "()V").as_deref(),
        Some("zz")
    );
    assert_eq!(maps.method("p/Sub", "shared", "()V").as_deref(), Some("zz"));
    // Чужого члена не выдумываем.
    assert_eq!(maps.method("p/Sub", "other", "()V"), None);
}
