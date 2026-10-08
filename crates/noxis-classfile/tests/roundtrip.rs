//! Round-trip `parse → write → parse` через backend.
//!
//! Пул при записи строится заново (порядок другой), поэтому сравнивается IR,
//! а не байты. Записанные байты обязаны проходить структурный `verify`
//! backend'а — это ловит битые ссылки пула и кривые атрибуты.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use noxis_classfile::{ClassFile, format};
use ristretto_classfile::attributes::{
    Attribute as RtAttr, BootstrapMethod as RtBsm, ExceptionTableEntry as RtEntry,
    Instruction as Rt, LineNumber as RtLine, StackFrame as RtFrame,
};
use ristretto_classfile::{
    ClassAccessFlags, ClassFile as RtClass, ConstantPool, Field as RtField, FieldAccessFlags,
    FieldType, JAVA_21, Method as RtMethod, MethodAccessFlags, ReferenceKind,
};

/// Минимальный класс: версия, флаги, поле с `ConstantValue`, `SourceFile`.
#[test]
fn minimal_class_round_trip() {
    let mut pool = ConstantPool::default();
    let this = pool.add_class("demo/Round").expect("add_class");
    let object = pool.add_class("java/lang/Object").expect("add_object");
    let name = pool.add_utf8("K").expect("name");
    let desc = pool.add_utf8("I").expect("desc");
    let value = pool.add_integer(42).expect("value");
    let source_name = pool.add_utf8("SourceFile").expect("attr");
    let source_file = pool.add_utf8("Round.java").expect("file");
    let constant_name = pool.add_utf8("ConstantValue").expect("attr");

    let rt = RtClass {
        version: JAVA_21,
        constant_pool: pool,
        access_flags: ClassAccessFlags::PUBLIC,
        this_class: this,
        super_class: object,
        interfaces: vec![],
        fields: vec![RtField {
            access_flags: FieldAccessFlags::PUBLIC
                | FieldAccessFlags::STATIC
                | FieldAccessFlags::FINAL,
            name_index: name,
            descriptor_index: desc,
            field_type: FieldType::parse("I").expect("field type"),
            attributes: vec![RtAttr::ConstantValue {
                name_index: constant_name,
                constant_value_index: value,
            }],
        }],
        methods: vec![],
        attributes: vec![RtAttr::SourceFile {
            name_index: source_name,
            source_file_index: source_file,
        }],
        ..RtClass::default()
    };

    let ir = round_trip(&rt);
    assert_eq!(ir.this_class, "demo/Round");
    assert_eq!(ir.super_class.as_deref(), Some("java/lang/Object"));
    assert_eq!(ir.methods.len(), 0);
    assert_eq!(ir.fields.len(), 1);
}

/// Метод с ветвлениями, handler'ом, номерами строк и кадрами.
#[test]
fn code_with_branches_round_trip() {
    let mut pool = ConstantPool::default();
    let this = pool.add_class("demo/Flow").expect("add_class");
    let object = pool.add_class("java/lang/Object").expect("add_object");
    let name = pool.add_utf8("run").expect("name");
    let desc = pool.add_utf8("()I").expect("desc");
    let code_name = pool.add_utf8("Code").expect("attr");
    let lines_name = pool.add_utf8("LineNumberTable").expect("attr");
    let frames_name = pool.add_utf8("StackMapTable").expect("attr");

    // Логические номера: 0 iconst_0, 1 istore_1, 2 iload_1, 3 ifeq→6,
    // 4 iconst_1, 5 goto→7, 6 iconst_2, 7 ireturn.
    let code = vec![
        Rt::Iconst_0,
        Rt::Istore_1,
        Rt::Iload_1,
        Rt::Ifeq(6),
        Rt::Iconst_1,
        Rt::Goto(7),
        Rt::Iconst_2,
        Rt::Ireturn,
    ];
    let code_attr = RtAttr::Code {
        name_index: code_name,
        max_stack: 1,
        max_locals: 2,
        code,
        exception_table: vec![RtEntry {
            range_pc: 0..4,
            handler_pc: 6,
            catch_type: 0,
        }],
        attributes: vec![
            RtAttr::LineNumberTable {
                name_index: lines_name,
                line_numbers: vec![
                    RtLine {
                        start_pc: 0,
                        line_number: 10,
                    },
                    RtLine {
                        start_pc: 6,
                        line_number: 20,
                    },
                ],
            },
            RtAttr::StackMapTable {
                name_index: frames_name,
                frames: vec![
                    RtFrame::SameFrame { frame_type: 6 },
                    RtFrame::SameFrame { frame_type: 0 },
                ],
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
            attributes: vec![code_attr],
        }],
        attributes: vec![],
        ..RtClass::default()
    };

    let ir = round_trip(&rt);
    let method = &ir.methods[0];
    let code = method.code.as_ref().expect("есть Code");
    assert_eq!(code.max_stack, 1);
    assert_eq!(code.max_locals, 2);
    // Номера инструкций: 0,1,2,3(ifeq),4,5(goto),6,7; кадры на 6 и 7.
    let frames = &code.frames;
    assert_eq!(frames.len(), 2);
    assert!(format!("{frames:?}").contains("offset_delta: 6"));
    assert_eq!(code.exception_table.len(), 1);
    assert_eq!(code.attributes.len(), 1);
}

/// `BootstrapMethods` + `invokedynamic` переживают round-trip.
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

/// `parse → write → parse` даёт тот же IR; записанное верифицируется.
fn round_trip(rt: &RtClass<'_>) -> ClassFile {
    let mut original = Vec::new();
    rt.to_bytes(&mut original).expect("to_bytes");
    let first = format::parse(&original).expect("parse");
    let written = format::write(&first).expect("write");
    RtClass::from_bytes(&written)
        .expect("written parses")
        .verify()
        .expect("written verifies");
    let second = format::parse(&written).expect("re-parse");
    assert_eq!(first, second, "IR обязан пережить round-trip");
    second
}
