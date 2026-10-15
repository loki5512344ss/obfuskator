//! `CONSTANT_Dynamic` (condy) round trips: the writer must be able to emit a
//! brand-new `ldc` on a `Const::Dynamic`, not just pass one through on read.
//!
//! Why this matters: a generic sandbox-based deobfuscator (narumii/SSVM)
//! confirmed empirically (`todo.md`, 2026-10-14) resolves plain arithmetic
//! and runtime-decrypt calls, but does not resolve `invokedynamic`/condy at
//! all - 0 changes on a condy-only test class. Strings/Constants moving to
//! condy is the first concrete fix that follows from that finding.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::super::round_trip;
use noxis_classfile::{
    Attribute, BootstrapMethod, ClassBuilder, Code, Const, Insn, Version, format,
};
use ristretto_classfile::attributes::{
    Attribute as RtAttr, BootstrapMethod as RtBsm, Instruction as Rt,
};
use ristretto_classfile::{
    ClassAccessFlags, ClassFile as RtClass, ConstantPool, JAVA_21, Method as RtMethod,
    MethodAccessFlags, ReferenceKind,
};

/// `CONSTANT_Dynamic` (condy, `ldc`) survives the round trip - needed for
/// passes that emit strings/constants through a bootstrap method instead of
/// a plain `ldc` literal.
#[test]
fn condy_ldc_round_trip() {
    let mut pool = ConstantPool::default();
    let this = pool.add_class("demo/Condy").expect("add_class");
    let object = pool.add_class("java/lang/Object").expect("add_object");
    let owner = pool.add_class("demo/RT").expect("owner");
    let method_ref = pool
        .add_method_ref(
            owner,
            "boot",
            "(Ljava/lang/invoke/MethodHandles$Lookup;Ljava/lang/String;Ljava/lang/Class;)Ljava/lang/String;",
        )
        .expect("ref");
    let handle = pool
        .add_method_handle(ReferenceKind::InvokeStatic, method_ref)
        .expect("handle");
    let bsm_name = pool.add_utf8("BootstrapMethods").expect("attr");
    let name = pool.add_utf8("run").expect("name");
    let desc = pool.add_utf8("()Ljava/lang/String;").expect("desc");
    let code_name = pool.add_utf8("Code").expect("attr");
    let dynamic = pool
        .add_dynamic(0, "_", "Ljava/lang/String;")
        .expect("condy");

    let rt = RtClass {
        version: JAVA_21,
        constant_pool: pool,
        access_flags: ClassAccessFlags::PUBLIC,
        this_class: this,
        super_class: object,
        interfaces: vec![],
        fields: vec![],
        methods: vec![RtMethod {
            access_flags: MethodAccessFlags::PUBLIC | MethodAccessFlags::STATIC,
            name_index: name,
            descriptor_index: desc,
            attributes: vec![RtAttr::Code {
                name_index: code_name,
                max_stack: 1,
                max_locals: 0,
                code: vec![Rt::Ldc_w(dynamic), Rt::Areturn],
                exception_table: vec![],
                attributes: vec![],
            }],
        }],
        attributes: vec![RtAttr::BootstrapMethods {
            name_index: bsm_name,
            methods: vec![RtBsm {
                bootstrap_method_ref: handle,
                arguments: vec![],
            }],
        }],
        ..RtClass::default()
    };

    let ir = round_trip(&rt);
    assert_eq!(ir.attributes.len(), 1);
    assert!(format!("{:?}", ir.attributes[0]).contains("BootstrapMethods"));
    let code = ir.methods[0].code.as_ref().expect("code");
    assert!(
        format!("{:?}", code.insns).contains("Dynamic"),
        "condy ldc должен пережить round-trip как Const::Dynamic: {:?}",
        code.insns
    );
}

/// Регрессия: писатель раньше эмитил `Const::Dynamic` ВСЕГДА через
/// `ldc2_w` (категория 2 - long/double, 2 слота стека), даже когда BSM
/// возвращает `String` (категория 1). `ldc2_w` на ссылочном типе -
/// категория инструкции не совпадает с категорией значения, кадры стека
/// и верификатор на реальной JVM это не пропустят. Категория должна
/// читаться из дескриптора condy-константы (JVMS §4.4.10), не быть
/// захардкожена.
#[test]
fn condy_string_writes_ldc_w_not_ldc2_w() {
    let class = ClassBuilder::new("demo/CondyOp", None)
        .version(Version::JAVA_21)
        .attribute(Attribute::BootstrapMethods(vec![BootstrapMethod {
            kind: 6, // InvokeStatic
            owner: "demo/RT".to_string(),
            name: "boot".to_string(),
            descriptor: "(Ljava/lang/invoke/MethodHandles$Lookup;Ljava/lang/String;Ljava/lang/Class;)Ljava/lang/String;"
                .to_string(),
            itf: false,
            args: vec![],
        }]))
        .method_with_code(
            0x0009,
            "run",
            "()Ljava/lang/String;",
            Code {
                max_stack: 1,
                max_locals: 0,
                insns: vec![
                    Insn::Const(Const::Dynamic {
                        name: "_".to_string(),
                        descriptor: "Ljava/lang/String;".to_string(),
                        bootstrap: 0,
                    }),
                    Insn::AReturn,
                ],
                exception_table: Vec::new(),
                frames: Vec::new(),
                attributes: Vec::new(),
            },
        )
        .build();

    let bytes = format::write(&class).expect("write");
    let written = RtClass::from_bytes(&bytes).expect("parse back");
    let method = &written.methods[0];
    let RtAttr::Code { code, .. } = method
        .attributes
        .iter()
        .find(|a| matches!(a, RtAttr::Code { .. }))
        .expect("Code attribute")
    else {
        unreachable!()
    };
    assert!(
        matches!(code.first(), Some(Rt::Ldc_w(_))),
        "condy String должен писаться через ldc_w (категория 1), не ldc2_w: {code:?}"
    );
    assert!(
        !code.iter().any(|insn| matches!(insn, Rt::Ldc2_w(_))),
        "ldc2_w не должен появляться для condy-строки: {code:?}"
    );
}
