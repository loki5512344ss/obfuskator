//! Attribute round trips: bootstrap methods, nesting, flags, sealing.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod condy;

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

/// Runtime-visible annotations on a method survive the round trip: the
/// pool is rebuilt on write, so raw index blobs would go stale and
/// reflection (`Method.getAnnotation`) would die with
/// `AnnotationFormatError`/`GenericSignatureFormatError`.
#[test]
fn runtime_visible_annotations_round_trip() {
    use ristretto_classfile::attributes::{
        Annotation as RtAnno, AnnotationElement as RtElem, AnnotationValuePair as RtPair,
    };
    let mut pool = ConstantPool::default();
    let this = pool.add_class("demo/Anno").expect("this");
    let object = pool.add_class("java/lang/Object").expect("object");
    let name = pool.add_utf8("run").expect("name");
    let desc = pool.add_utf8("()V").expect("desc");
    let code_name = pool.add_utf8("Code").expect("code");
    let attr_name = pool.add_utf8("RuntimeVisibleAnnotations").expect("rva");
    let marker = pool.add_utf8("Ldemo/Marker;").expect("marker");
    let value_key = pool.add_utf8("value").expect("value key");
    let hello = pool.add_utf8("hello").expect("hello");
    let klass_key = pool.add_utf8("klass").expect("klass key");
    let target = pool.add_utf8("Ldemo/Target;").expect("target");
    let kind_key = pool.add_utf8("kind").expect("kind key");
    let kind_type = pool.add_utf8("Ldemo/Kind;").expect("kind type");
    let one = pool.add_utf8("ONE").expect("one");
    let nested_key = pool.add_utf8("nested").expect("nested key");
    let nested_type = pool.add_utf8("Ldemo/Nested;").expect("nested type");
    let arr_key = pool.add_utf8("arr").expect("arr key");
    let seven = pool.add_integer(7).expect("seven");

    let annotation = RtAnno {
        type_index: marker,
        elements: vec![
            RtPair {
                name_index: value_key,
                value: RtElem::String {
                    const_value_index: hello,
                },
            },
            RtPair {
                name_index: klass_key,
                value: RtElem::Class {
                    class_info_index: target,
                },
            },
            RtPair {
                name_index: kind_key,
                value: RtElem::Enum {
                    type_name_index: kind_type,
                    const_name_index: one,
                },
            },
            RtPair {
                name_index: nested_key,
                value: RtElem::Annotation {
                    annotation: RtAnno {
                        type_index: nested_type,
                        elements: vec![],
                    },
                },
            },
            RtPair {
                name_index: arr_key,
                value: RtElem::Array {
                    values: vec![
                        RtElem::String {
                            const_value_index: hello,
                        },
                        RtElem::Int {
                            const_value_index: seven,
                        },
                    ],
                },
            },
        ],
    };

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
            attributes: vec![
                RtAttr::Code {
                    name_index: code_name,
                    max_stack: 0,
                    max_locals: 1,
                    code: vec![Rt::Return],
                    exception_table: vec![],
                    attributes: vec![],
                },
                RtAttr::RuntimeVisibleAnnotations {
                    name_index: attr_name,
                    annotations: vec![annotation],
                },
            ],
        }],
        attributes: vec![],
        ..RtClass::default()
    };

    let ir = round_trip(&rt);
    let attrs = &ir.methods[0].attributes;
    let annos = attrs
        .iter()
        .find_map(|a| match a {
            noxis_classfile::Attribute::RuntimeVisibleAnnotations(list) => Some(list),
            _ => None,
        })
        .expect("annotations modeled");
    assert_eq!(annos.len(), 1);
    assert_eq!(annos[0].descriptor, "Ldemo/Marker;");
    assert_eq!(annos[0].elements.len(), 5);
}
