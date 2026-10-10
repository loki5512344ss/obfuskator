//! Reference rewriting: array operands and generic signatures.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::{run_rename, sample};
use noxis_classfile::Insn;
/// Array type operands (`checkcast`, `instanceof`, `anewarray`,
/// `multianewarray`) carry descriptors, not plain names: the element class
/// inside must follow the mapping, or the reference silently goes stale
/// (and the next frame computation merges against a ghost type).
#[test]
fn array_type_operands_follow_their_element() {
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
    code.insns.extend([
        Insn::CheckCast("[Lapp/Helper;".to_string()),
        Insn::Pop,
        Insn::InstanceOf("[Lapp/Helper;".to_string()),
        Insn::Pop,
        Insn::ANewArray("app/Helper".to_string()),
        Insn::Pop,
        Insn::MultiANewArray {
            descriptor: "[[Lapp/Helper;".to_string(),
            dims: 2,
        },
        Insn::Pop,
    ]);
    let ctx = run_rename(&mut program);
    let helper = ctx.mapping.class("app/Helper").expect("renamed");
    let main = program.class("app/Main").expect("kept");
    let method = main
        .methods
        .iter()
        .find(|m| m.name == "main")
        .expect("main");
    let insns = &method.code.as_ref().expect("code").insns;
    let cast = insns
        .iter()
        .find_map(|i| match i {
            Insn::CheckCast(name) => Some(name),
            _ => None,
        })
        .expect("checkcast");
    assert_eq!(cast, &format!("[L{helper};"));
    let inst = insns
        .iter()
        .find_map(|i| match i {
            Insn::InstanceOf(name) => Some(name),
            _ => None,
        })
        .expect("instanceof");
    assert_eq!(inst, &format!("[L{helper};"));
    let anew = insns
        .iter()
        .find_map(|i| match i {
            Insn::ANewArray(name) => Some(name),
            _ => None,
        })
        .expect("anewarray");
    assert_eq!(anew, helper);
    let multi = insns
        .iter()
        .find_map(|i| match i {
            Insn::MultiANewArray { descriptor, .. } => Some(descriptor),
            _ => None,
        })
        .expect("multianewarray");
    assert_eq!(multi, &format!("[[L{helper};"));
}

/// Generic `Signature`s rewrite every referenced class, not just the first:
/// a stale `Comparable<Old>` leaks the original name and breaks reflective
/// generic resolution at runtime.
#[test]
fn generic_signatures_rewrite_all_references() {
    use noxis_classfile::Attribute;
    let mut program = sample();
    let main = program
        .classes
        .iter_mut()
        .find(|c| c.this_class == "app/Main")
        .expect("main");
    main.attributes.push(Attribute::Signature(
        "Ljava/lang/Object;Ljava/lang/Comparable<Lapp/Helper;>;".to_string(),
    ));
    let ctx = run_rename(&mut program);
    let helper = ctx.mapping.class("app/Helper").expect("renamed");
    let main = program.class("app/Main").expect("kept");
    let signature = main
        .attributes
        .iter()
        .find_map(|attr| match attr {
            Attribute::Signature(text) => Some(text),
            _ => None,
        })
        .expect("signature");
    assert_eq!(
        signature,
        &format!("Ljava/lang/Object;Ljava/lang/Comparable<L{helper};>;")
    );
}

/// Sealed classes keep working: `PermittedSubclasses` entries follow the
/// mapping (a stale entry breaks loading with `ClassFormatError`, and raw
/// pool indices would go stale on rebuild anyway).
#[test]
fn permitted_subclasses_follow_the_mapping() {
    use noxis_classfile::{Attribute, ClassBuilder, Version};
    let mut program = sample();
    program.classes.push(
        ClassBuilder::new("app/Sealed", Some("java/lang/Object"))
            .version(Version::JAVA_21)
            .attribute(Attribute::PermittedSubclasses(vec![
                "app/Helper".to_string(),
            ]))
            .build(),
    );
    let ctx = run_rename(&mut program);
    let helper = ctx.mapping.class("app/Helper").expect("renamed");
    let sealed_name = ctx.mapping.class("app/Sealed").expect("renamed");
    let sealed = program.class(sealed_name).expect("sealed");
    let permitted = sealed
        .attributes
        .iter()
        .find_map(|attr| match attr {
            Attribute::PermittedSubclasses(names) => Some(names),
            _ => None,
        })
        .expect("permitted");
    assert_eq!(permitted, &vec![helper.to_string()]);
}
