//! Type annotations and `AnnotationDefault` must be modeled, not passed
//! through as raw bytes: the writer rebuilds the pool, so stale indices
//! point at the wrong utf8 or past the pool end. ASM dies on those with
//! `ArrayIndexOutOfBoundsException` (Paper's plugin converter on real jars).

#![allow(clippy::expect_used, clippy::unwrap_used)]

use noxis_classfile::format;
use ristretto_classfile::attributes::{
    AnnotationElement as RtElem, AnnotationValuePair as RtPair, Attribute as RtAttr,
    Instruction as Rt, TargetType as RtTarget, TypeAnnotation as RtTypeAnno,
};
use ristretto_classfile::{
    ClassAccessFlags, ClassFile as RtClass, ConstantPool, JAVA_21, Method as RtMethod,
    MethodAccessFlags,
};

/// Method-level `RuntimeVisibleTypeAnnotations` (target `0x14`, method
/// return type): after the write rebuilds the pool, `type_index` and the
/// element references must resolve to the same strings as before.
#[test]
fn method_type_annotation_index_is_rebuilt() {
    let mut pool = ConstantPool::default();
    let this = pool.add_class("demo/TA").expect("this");
    let object = pool.add_class("java/lang/Object").expect("object");
    let name = pool.add_utf8("run").expect("name");
    let desc = pool.add_utf8("()Ljava/lang/Object;").expect("desc");
    let attr = pool
        .add_utf8("RuntimeVisibleTypeAnnotations")
        .expect("attr");
    let type_desc = pool.add_utf8("Ldemo/Use;").expect("type");
    let value_name = pool.add_utf8("value").expect("value");
    let enum_desc = pool.add_utf8("Ldemo/Kind;").expect("enum desc");
    let enum_const = pool.add_utf8("A").expect("enum const");

    let rt = RtClass {
        version: JAVA_21,
        constant_pool: pool,
        access_flags: ClassAccessFlags::PUBLIC | ClassAccessFlags::SUPER,
        this_class: this,
        super_class: object,
        interfaces: vec![],
        fields: vec![],
        methods: vec![RtMethod {
            access_flags: MethodAccessFlags::PUBLIC | MethodAccessFlags::ABSTRACT,
            name_index: name,
            descriptor_index: desc,
            attributes: vec![RtAttr::RuntimeVisibleTypeAnnotations {
                name_index: attr,
                type_annotations: vec![RtTypeAnno {
                    target_type: RtTarget::Empty { target_type: 0x14 },
                    type_path: vec![],
                    type_index: type_desc,
                    elements: vec![RtPair {
                        name_index: value_name,
                        value: RtElem::Enum {
                            type_name_index: enum_desc,
                            const_name_index: enum_const,
                        },
                    }],
                }],
            }],
        }],
        attributes: vec![],
        ..RtClass::default()
    };

    let written = rewritten(&rt);
    let back = RtClass::from_bytes(&written).expect("reparse");
    let anno = back.methods[0]
        .attributes
        .iter()
        .find_map(|a| match a {
            RtAttr::RuntimeVisibleTypeAnnotations {
                type_annotations, ..
            } => type_annotations.first(),
            _ => None,
        })
        .expect("type annotation modeled, not Raw");
    let resolved = back
        .constant_pool
        .try_get_utf8(anno.type_index)
        .expect("type_index valid")
        .to_string();
    assert_eq!(resolved, "Ldemo/Use;");
    let pair = anno.elements.first().expect("one element pair");
    let pair_name = back
        .constant_pool
        .try_get_utf8(pair.name_index)
        .expect("element name valid")
        .to_string();
    assert_eq!(pair_name, "value");
    let RtElem::Enum {
        type_name_index,
        const_name_index,
    } = &pair.value
    else {
        panic!("enum element stays an enum");
    };
    assert_eq!(
        back.constant_pool
            .try_get_utf8(*type_name_index)
            .expect("enum type valid")
            .to_string(),
        "Ldemo/Kind;"
    );
    assert_eq!(
        back.constant_pool
            .try_get_utf8(*const_name_index)
            .expect("enum const valid")
            .to_string(),
        "A"
    );
}

/// Code-level `RuntimeInvisibleTypeAnnotations` (cast type argument,
/// target `0x48`): the attribute lives inside `Code`, same rebuild rule.
#[test]
fn code_type_annotation_index_is_rebuilt() {
    let mut pool = ConstantPool::default();
    let this = pool.add_class("demo/CTA").expect("this");
    let object = pool.add_class("java/lang/Object").expect("object");
    let name = pool.add_utf8("run").expect("name");
    let desc = pool.add_utf8("()V").expect("desc");
    let code_name = pool.add_utf8("Code").expect("code");
    let attr = pool
        .add_utf8("RuntimeInvisibleTypeAnnotations")
        .expect("attr");
    let type_desc = pool.add_utf8("Llocal/Use;").expect("type");

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
                max_stack: 1,
                max_locals: 0,
                code: vec![Rt::Aconst_null, Rt::Pop, Rt::Return],
                exception_table: vec![],
                attributes: vec![RtAttr::RuntimeInvisibleTypeAnnotations {
                    name_index: attr,
                    type_annotations: vec![RtTypeAnno {
                        target_type: RtTarget::TypeArgument {
                            target_type: 0x48,
                            offset: 0,
                            type_argument_index: 0,
                        },
                        type_path: vec![],
                        type_index: type_desc,
                        elements: vec![],
                    }],
                }],
            }],
        }],
        attributes: vec![],
        ..RtClass::default()
    };

    let written = rewritten(&rt);
    let back = RtClass::from_bytes(&written).expect("reparse");
    let code = back.methods[0]
        .attributes
        .iter()
        .find_map(|a| match a {
            RtAttr::Code { attributes, .. } => Some(attributes),
            _ => None,
        })
        .expect("code");
    let anno = code
        .iter()
        .find_map(|a| match a {
            RtAttr::RuntimeInvisibleTypeAnnotations {
                type_annotations, ..
            } => type_annotations.first(),
            _ => None,
        })
        .expect("type annotation modeled, not Raw");
    let resolved = back
        .constant_pool
        .try_get_utf8(anno.type_index)
        .expect("type_index valid")
        .to_string();
    assert_eq!(resolved, "Llocal/Use;");
}

/// `AnnotationDefault` holds one `element_value` with pool references -
/// same stale-index hazard as the annotations themselves.
#[test]
fn annotation_default_index_is_rebuilt() {
    let mut pool = ConstantPool::default();
    let this = pool.add_class("demo/Def").expect("this");
    let object = pool.add_class("java/lang/Object").expect("object");
    let name = pool.add_utf8("value").expect("name");
    let desc = pool.add_utf8("()Ljava/lang/String;").expect("desc");
    let attr = pool.add_utf8("AnnotationDefault").expect("attr");
    let value = pool.add_utf8("x").expect("value");

    let rt = RtClass {
        version: JAVA_21,
        constant_pool: pool,
        access_flags: ClassAccessFlags::PUBLIC
            | ClassAccessFlags::INTERFACE
            | ClassAccessFlags::ABSTRACT,
        this_class: this,
        super_class: object,
        interfaces: vec![],
        fields: vec![],
        methods: vec![RtMethod {
            access_flags: MethodAccessFlags::PUBLIC | MethodAccessFlags::ABSTRACT,
            name_index: name,
            descriptor_index: desc,
            attributes: vec![RtAttr::AnnotationDefault {
                name_index: attr,
                element: RtElem::String {
                    const_value_index: value,
                },
            }],
        }],
        attributes: vec![],
        ..RtClass::default()
    };

    let written = rewritten(&rt);
    let back = RtClass::from_bytes(&written).expect("reparse");
    let element = back.methods[0]
        .attributes
        .iter()
        .find_map(|a| match a {
            RtAttr::AnnotationDefault { element, .. } => Some(element),
            _ => None,
        })
        .expect("AnnotationDefault modeled, not Raw");
    let RtElem::String { const_value_index } = element else {
        panic!("default stays a string element");
    };
    let resolved = back
        .constant_pool
        .try_get_utf8(*const_value_index)
        .expect("const_value_index valid")
        .to_string();
    assert_eq!(resolved, "x");
}

/// `parse → write` via our IR; the rewritten bytes for re-parsing.
fn rewritten(rt: &RtClass<'_>) -> Vec<u8> {
    let mut original = Vec::new();
    rt.to_bytes(&mut original).expect("to_bytes");
    let ir = format::parse(&original).expect("parse");
    format::write(&ir).expect("write")
}
