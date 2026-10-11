//! Local-class attributes (`EnclosingMethod`) and array-owner references.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::{run_rename, sample};
use noxis_classfile::Insn;

/// `EnclosingMethod` (local/anonymous classes) keeps a class + optional
/// method pair: a stale class here breaks reflection on the local class
/// and, with certain enum patterns, class initialization itself.
#[test]
fn enclosing_method_follows_the_mapping() {
    use noxis_classfile::{Attribute, ClassBuilder, Version};
    let mut program = sample();
    program.classes.push(
        ClassBuilder::new("app/Local", Some("java/lang/Object"))
            .version(Version::JAVA_21)
            .method_with_code(0x000a, "go", "()V", noxis_classfile::empty_code(0))
            .build(),
    );
    let local = program
        .classes
        .iter_mut()
        .find(|c| c.this_class == "app/Local")
        .expect("local");
    local.attributes.push(Attribute::EnclosingMethod {
        class: "app/Helper".to_string(),
        method: Some(("make".to_string(), "()Lapp/Helper;".to_string())),
    });
    let ctx = run_rename(&mut program);
    let helper = ctx.mapping.class("app/Helper").expect("renamed");
    let local_name = ctx.mapping.class("app/Local").expect("local renamed");
    let local = program.class(local_name).expect("renamed class");
    let attr = local
        .attributes
        .iter()
        .find_map(|a| match a {
            Attribute::EnclosingMethod { class, method } => Some((class, method)),
            _ => None,
        })
        .expect("enclosing");
    assert_eq!(attr.0, helper);
    let (name, desc) = attr.1.as_ref().expect("method");
    assert_ne!(name, "make");
    assert_eq!(desc, &format!("()L{helper};"));
}

/// `invokevirtual` on an array (`[Lpkg/C;`.clone) names the *array* class
/// as owner: the mapping key is the element class, so a plain lookup misses
/// and the array component goes stale - enum `values()` clones break.
#[test]
fn array_owner_method_refs_follow_their_element() {
    use noxis_classfile::InvokeKind;
    let mut program = sample();
    let main = program
        .classes
        .iter_mut()
        .find(|c| c.this_class == "app/Main")
        .expect("main");
    let probe = main
        .methods
        .iter_mut()
        .find(|m| m.name == "main")
        .expect("main method");
    let code = probe.code.as_mut().expect("code");
    code.insns.push(Insn::Invoke {
        kind: InvokeKind::Virtual,
        owner: "[Lapp/Helper;".to_string(),
        name: "clone".to_string(),
        descriptor: "()Ljava/lang/Object;".to_string(),
        itf: false,
    });
    code.insns.push(Insn::Pop);
    let ctx = run_rename(&mut program);
    let helper = ctx.mapping.class("app/Helper").expect("renamed");
    let main = program.class("app/Main").expect("kept");
    let method = main
        .methods
        .iter()
        .find(|m| m.name == "main")
        .expect("main");
    let invoke = method
        .code
        .as_ref()
        .expect("code")
        .insns
        .iter()
        .find_map(|i| match i {
            Insn::Invoke { owner, name, .. } if name == "clone" => Some(owner),
            _ => None,
        })
        .expect("invoke");
    assert_eq!(invoke, &format!("[L{helper};"));
}
