//! Attribute round trips: bootstrap methods, nesting, flags, sealing.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::round_trip;
use ristretto_classfile::attributes::{
    Attribute as RtAttr, BootstrapMethod as RtBsm, Instruction as Rt,
};
use ristretto_classfile::{
    ClassAccessFlags, ClassFile as RtClass, ConstantPool, JAVA_21, Method as RtMethod,
    MethodAccessFlags, ReferenceKind,
};
/// `BootstrapMethods` + `invokedynamic` survive the round trip.
#[test]
fn bootstrap_methods_round_trip() {
    let mut pool = ConstantPool::default();
    let this = pool.add_class("demo/Lambda").expect("add_class");
    let object = pool.add_class("java/lang/Object").expect("add_object");
    let owner = pool.add_class("demo/RT").expect("owner");
    let method_ref = pool.add_method_ref(owner, "boot", "()V").expect("ref");
    let handle = pool
        .add_method_handle(ReferenceKind::InvokeStatic, method_ref)
        .expect("handle");
    let arg = pool.add_string("x").expect("arg");
    let bsm_name = pool.add_utf8("BootstrapMethods").expect("attr");
    let name = pool.add_utf8("run").expect("name");
    let desc = pool.add_utf8("()V").expect("desc");
    let code_name = pool.add_utf8("Code").expect("attr");
    let dynamic = pool.add_invoke_dynamic(0, "run", "()V").expect("indy");

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
                code: vec![Rt::Invokedynamic(dynamic), Rt::Return],
                exception_table: vec![],
                attributes: vec![],
            }],
        }],
        attributes: vec![RtAttr::BootstrapMethods {
            name_index: bsm_name,
            methods: vec![RtBsm {
                bootstrap_method_ref: handle,
                arguments: vec![arg],
            }],
        }],
        ..RtClass::default()
    };

    let ir = round_trip(&rt);
    assert_eq!(ir.attributes.len(), 1);
    assert!(format!("{:?}", ir.attributes[0]).contains("BootstrapMethods"));
}

/// `NestMembers` переживает пересборку пула (баг: `ClassFormatError`).
#[test]
fn nest_attributes_round_trip() {
    let mut pool = ConstantPool::default();
    let this = pool.add_class("demo/Nest").expect("c");
    let object = pool.add_class("java/lang/Object").expect("o");
    let member = pool.add_class("demo/Nest$Inner").expect("m");
    let mn = pool.add_utf8("NestMembers").expect("n");
    let mems = RtAttr::NestMembers {
        name_index: mn,
        class_indexes: vec![this, member],
    };
    let rt = RtClass {
        version: JAVA_21,
        constant_pool: pool,
        this_class: this,
        super_class: object,
        attributes: vec![mems],
        ..RtClass::default()
    };
    let ir = round_trip(&rt);
    assert!(format!("{:?}", ir.attributes).contains("NestMembers"));
}

/// `parse → write → parse` даёт тот же IR; записанное верифицируется.
#[test]
fn bare_interface_flags_normalize_on_parse() {
    use ristretto_classfile::ClassAccessFlags;
    let mut pool = ConstantPool::default();
    let this = pool.add_class("demo/Bare").expect("add_class");
    let object = pool.add_class("java/lang/Object").expect("add_object");
    let rt = RtClass {
        version: JAVA_21,
        constant_pool: pool,
        access_flags: ClassAccessFlags::INTERFACE,
        this_class: this,
        super_class: object,
        interfaces: vec![],
        fields: vec![],
        methods: vec![],
        attributes: vec![],
        ..RtClass::default()
    };
    let ir = round_trip(&rt);
    assert_eq!(ir.access_flags & 0x0200, 0x0200, "still an interface");
    assert_eq!(
        ir.access_flags & 0x0400,
        0x0400,
        "abstract is implied for interfaces"
    );
}

/// `PermittedSubclasses` survives the round trip: its pool indices are
/// remapped on write (not copied verbatim), or the backend rejects the
/// rebuilt pool (`bad constant type`).
#[test]
fn permitted_subclasses_round_trip() {
    let mut pool = ConstantPool::default();
    let this = pool.add_class("demo/Sealed").expect("add_class");
    let object = pool.add_class("java/lang/Object").expect("add_object");
    let permitted = pool.add_class("demo/Impl").expect("add permitted");
    let attr_name = pool.add_utf8("PermittedSubclasses").expect("attr");
    let rt = RtClass {
        version: JAVA_21,
        constant_pool: pool,
        access_flags: ClassAccessFlags::PUBLIC | ClassAccessFlags::ABSTRACT,
        this_class: this,
        super_class: object,
        interfaces: vec![],
        fields: vec![],
        methods: vec![],
        attributes: vec![RtAttr::PermittedSubclasses {
            name_index: attr_name,
            class_indexes: vec![permitted],
        }],
        ..RtClass::default()
    };
    let ir = round_trip(&rt);
    assert_eq!(ir.this_class, "demo/Sealed");
}
