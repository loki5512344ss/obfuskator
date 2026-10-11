//! Code-level round trips that the IR must survive.

use noxis_classfile::Insn;
use noxis_classfile::format;
use ristretto_classfile::attributes::{Attribute as RtAttr, Instruction as Rt};
use ristretto_classfile::{
    ClassAccessFlags, ClassFile as RtClass, ConstantPool, JAVA_21, Method as RtMethod,
    MethodAccessFlags,
};

/// `invokestatic` on an *interface* method (`List.of`) needs a
/// `CONSTANT_InterfaceMethodref`: writing a plain `Methodref` passes the
/// verifier's structural check but HotSpot rejects the linkage with
/// `IncompatibleClassChangeError` at first call.
#[test]
fn invokestatic_interface_method_ref_round_trip() {
    let mut pool = ConstantPool::default();
    let this = pool.add_class("demo/IfStatic").expect("this");
    let object = pool.add_class("java/lang/Object").expect("object");
    let name = pool.add_utf8("main").expect("name");
    let desc = pool.add_utf8("([Ljava/lang/String;)V").expect("desc");
    let code_name = pool.add_utf8("Code").expect("code name");
    let list_class = pool.add_class("java/util/List").expect("list");
    let of = pool
        .add_interface_method_ref(
            list_class,
            "of",
            "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/util/List;",
        )
        .expect("interface ref");

    let code = vec![
        Rt::Aload_0,
        Rt::Aload_0,
        Rt::Invokestatic(of),
        Rt::Pop,
        Rt::Return,
    ];
    let rt = RtClass {
        version: JAVA_21,
        constant_pool: pool,
        access_flags: ClassAccessFlags::PUBLIC | ClassAccessFlags::SUPER,
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
                max_stack: 2,
                max_locals: 1,
                code,
                exception_table: vec![],
                attributes: vec![],
            }],
        }],
        attributes: vec![],
        ..RtClass::default()
    };

    let mut original = Vec::new();
    rt.to_bytes(&mut original).expect("to_bytes");
    let parsed = format::parse(&original).expect("parse");
    let method = &parsed.methods[0];
    let invoke = method
        .code
        .as_ref()
        .expect("code")
        .insns
        .iter()
        .find_map(|i| match i {
            Insn::Invoke {
                kind, owner, itf, ..
            } => Some((*kind, owner.clone(), *itf)),
            _ => None,
        })
        .expect("invokestatic");
    assert!(invoke.2, "parse must keep the interface flag");

    let written = format::write(&parsed).expect("write");
    let reparsed = format::parse(&written).expect("re-parse");
    let method = &reparsed.methods[0];
    let invoke = method
        .code
        .as_ref()
        .expect("code")
        .insns
        .iter()
        .find_map(|i| match i {
            Insn::Invoke {
                kind, owner, itf, ..
            } => Some((*kind, owner.clone(), *itf)),
            _ => None,
        })
        .expect("invokestatic");
    assert_eq!(
        invoke,
        (
            noxis_classfile::InvokeKind::Static,
            "java/util/List".to_string(),
            true
        ),
        "InterfaceMethodref must survive write"
    );
}
