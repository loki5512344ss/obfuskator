//! Resolving constant-pool indices into strings of our IR.

use ristretto_classfile::{Constant, ConstantPool, ReferenceKind};

use crate::error::ClassfileError;
use crate::insn::{Const, InvokeKind};
use crate::model::{Annotation, AnnotationValue};

/// The class name from a `CONSTANT_Class`.
pub(super) fn class_name(pool: &ConstantPool<'_>, index: u16) -> Result<String, ClassfileError> {
    let name = pool
        .try_get_class(index)
        .map_err(|e| ClassfileError::Parse(e.to_string()))?;
    Ok(name.to_string())
}

/// `(owner, name, descriptor)` for a `CONSTANT_Fieldref`/`_Methodref`.
pub(super) fn ref_parts(
    pool: &ConstantPool<'_>,
    index: u16,
) -> Result<(String, String, String), ClassfileError> {
    let constant = pool
        .try_get(index)
        .map_err(|e| ClassfileError::Parse(e.to_string()))?;
    let (class_index, name_and_type_index) = match constant {
        Constant::FieldRef {
            class_index,
            name_and_type_index,
        }
        | Constant::MethodRef {
            class_index,
            name_and_type_index,
        }
        | Constant::InterfaceMethodRef {
            class_index,
            name_and_type_index,
        } => (*class_index, *name_and_type_index),
        other => {
            return Err(ClassfileError::Parse(format!(
                "ожидалась ссылка на поле/метод, найден {other:?}"
            )));
        }
    };
    let owner = class_name(pool, class_index)?;
    let (name, descriptor) = name_and_type(pool, name_and_type_index)?;
    Ok((owner, name, descriptor))
}

/// `(name, descriptor)` for a `CONSTANT_NameAndType`.
pub(super) fn name_and_type(
    pool: &ConstantPool<'_>,
    index: u16,
) -> Result<(String, String), ClassfileError> {
    let (name_index, descriptor_index) = pool
        .try_get_name_and_type(index)
        .map_err(|e| ClassfileError::Parse(e.to_string()))?;
    let name = utf8(pool, *name_index)?;
    let descriptor = utf8(pool, *descriptor_index)?;
    Ok((name, descriptor))
}

/// The string from a `CONSTANT_Utf8`.
pub(super) fn utf8(pool: &ConstantPool<'_>, index: u16) -> Result<String, ClassfileError> {
    let value = pool
        .try_get_utf8(index)
        .map_err(|e| ClassfileError::Parse(e.to_string()))?;
    Ok(value.to_string())
}

/// The constant for `ldc`/`ldc_w`/`ldc2_w`.
pub(super) fn const_from(pool: &ConstantPool<'_>, index: u16) -> Result<Const, ClassfileError> {
    let constant = pool
        .try_get(index)
        .map_err(|e| ClassfileError::Parse(e.to_string()))?;
    Ok(match constant {
        Constant::Integer(v) => Const::Int(*v),
        Constant::Float(v) => Const::Float(*v),
        Constant::Long(v) => Const::Long(*v),
        Constant::Double(v) => Const::Double(*v),
        Constant::String(v) => Const::String(utf8(pool, *v)?),
        Constant::Class(v) => Const::Class(utf8(pool, *v)?),
        Constant::MethodType(v) => Const::MethodType(utf8(pool, *v)?),
        Constant::MethodHandle {
            reference_kind,
            reference_index,
        } => Const::MethodHandle {
            kind: kind_code(reference_kind),
            owner: ref_parts(pool, *reference_index)?.0,
            name: ref_parts(pool, *reference_index)?.1,
            descriptor: ref_parts(pool, *reference_index)?.2,
            itf: matches!(
                pool.try_get(*reference_index),
                Ok(Constant::InterfaceMethodRef { .. })
            ),
        },
        Constant::Dynamic {
            bootstrap_method_attr_index,
            name_and_type_index,
        } => {
            let (name, descriptor) = name_and_type(pool, *name_and_type_index)?;
            Const::Dynamic {
                name,
                descriptor,
                bootstrap: *bootstrap_method_attr_index,
            }
        }
        other => {
            return Err(ClassfileError::Parse(format!(
                "константа {other:?} недопустима в ldc"
            )));
        }
    })
}

/// `(name, descriptor, bootstrap)` for a `CONSTANT_InvokeDynamic`.
pub(super) fn dynamic_parts(
    pool: &ConstantPool<'_>,
    index: u16,
) -> Result<(String, String, u16), ClassfileError> {
    let constant = pool
        .try_get(index)
        .map_err(|e| ClassfileError::Parse(e.to_string()))?;
    let (bootstrap, name_and_type_index) = match constant {
        Constant::InvokeDynamic {
            bootstrap_method_attr_index,
            name_and_type_index,
        } => (*bootstrap_method_attr_index, *name_and_type_index),
        other => {
            return Err(ClassfileError::Parse(format!(
                "ожидалась InvokeDynamic, найден {other:?}"
            )));
        }
    };
    let (name, descriptor) = name_and_type(pool, name_and_type_index)?;
    Ok((name, descriptor, bootstrap))
}

/// Call kind by reference index: interface or regular.
pub(super) fn is_interface(pool: &ConstantPool<'_>, index: u16) -> bool {
    matches!(pool.try_get(index), Ok(Constant::InterfaceMethodRef { .. }))
}

/// JVMS §5.4.3.5: the `reference_kind` number for a `CONSTANT_MethodHandle`.
pub(super) fn kind_code(kind: &ReferenceKind) -> u8 {
    match kind {
        ReferenceKind::GetField => 1,
        ReferenceKind::GetStatic => 2,
        ReferenceKind::PutField => 3,
        ReferenceKind::PutStatic => 4,
        ReferenceKind::InvokeVirtual => 5,
        ReferenceKind::InvokeStatic => 6,
        ReferenceKind::InvokeSpecial => 7,
        ReferenceKind::NewInvokeSpecial => 8,
        ReferenceKind::InvokeInterface => 9,
    }
}

/// Checks that an `Invoke` really refers to the expected kind.
///
/// The JVM accepts both `Methodref` and `InterfaceMethodref` for every call
/// kind except `invokeinterface` (interface reference only there); being
/// stricter than the JVM breaks reading real classes (interface static methods).
pub(super) fn expect_kind(
    pool: &ConstantPool<'_>,
    index: u16,
    kind: InvokeKind,
) -> Result<(), ClassfileError> {
    let constant = pool
        .try_get(index)
        .map_err(|e| ClassfileError::Parse(e.to_string()))?;
    let expected = match kind {
        InvokeKind::Interface => matches!(constant, Constant::InterfaceMethodRef { .. }),
        _ => matches!(
            constant,
            Constant::MethodRef { .. } | Constant::InterfaceMethodRef { .. }
        ),
    };
    if expected {
        Ok(())
    } else {
        Err(ClassfileError::Parse(format!(
            "вызов {kind:?} указывает на несовместимую ссылку"
        )))
    }
}

/// One `annotation` structure → IR (names, not pool indices).
pub(super) fn annotation(
    pool: &ConstantPool<'_>,
    anno: &ristretto_classfile::attributes::Annotation,
) -> Result<Annotation, ClassfileError> {
    Ok(Annotation {
        descriptor: utf8(pool, anno.type_index)?,
        elements: anno
            .elements
            .iter()
            .map(|pair| Ok((utf8(pool, pair.name_index)?, element(pool, &pair.value)?)))
            .collect::<Result<_, ClassfileError>>()?,
    })
}

/// One `element_value` → IR, keeping the tag (HotSpot checks it).
fn element(
    pool: &ConstantPool<'_>,
    value: &ristretto_classfile::attributes::AnnotationElement,
) -> Result<AnnotationValue, ClassfileError> {
    use ristretto_classfile::attributes::AnnotationElement as E;
    Ok(match value {
        E::Byte { const_value_index } => {
            AnnotationValue::Byte(const_int(pool, *const_value_index)?)
        }
        E::Char { const_value_index } => {
            AnnotationValue::Char(const_int(pool, *const_value_index)?)
        }
        E::Short { const_value_index } => {
            AnnotationValue::Short(const_int(pool, *const_value_index)?)
        }
        E::Boolean { const_value_index } => {
            AnnotationValue::Boolean(const_int(pool, *const_value_index)?)
        }
        E::Int { const_value_index } => AnnotationValue::Int(const_int(pool, *const_value_index)?),
        E::Long { const_value_index } => {
            AnnotationValue::Long(const_long(pool, *const_value_index)?)
        }
        E::Float { const_value_index } => {
            AnnotationValue::Float(const_float(pool, *const_value_index)?)
        }
        E::Double { const_value_index } => {
            AnnotationValue::Double(const_double(pool, *const_value_index)?)
        }
        E::String { const_value_index } => AnnotationValue::String(utf8(pool, *const_value_index)?),
        E::Enum {
            type_name_index,
            const_name_index,
        } => AnnotationValue::Enum {
            descriptor: utf8(pool, *type_name_index)?,
            name: utf8(pool, *const_name_index)?,
        },
        E::Class { class_info_index } => AnnotationValue::Class(utf8(pool, *class_info_index)?),
        E::Annotation { annotation: nested } => {
            AnnotationValue::Nested(Box::new(annotation(pool, nested)?))
        }
        E::Array { values } => AnnotationValue::Array(
            values
                .iter()
                .map(|value| element(pool, value))
                .collect::<Result<_, _>>()?,
        ),
    })
}

/// The `CONSTANT_Integer` behind a `B`/`C`/`S`/`Z`/`I` element.
fn const_int(pool: &ConstantPool<'_>, index: u16) -> Result<i32, ClassfileError> {
    match const_from(pool, index)? {
        Const::Int(v) => Ok(v),
        _ => Err(ClassfileError::Parse("не Integer".into())),
    }
}

fn const_long(pool: &ConstantPool<'_>, index: u16) -> Result<i64, ClassfileError> {
    match const_from(pool, index)? {
        Const::Long(v) => Ok(v),
        _ => Err(ClassfileError::Parse("не Long".into())),
    }
}

fn const_float(pool: &ConstantPool<'_>, index: u16) -> Result<f32, ClassfileError> {
    match const_from(pool, index)? {
        Const::Float(v) => Ok(v),
        _ => Err(ClassfileError::Parse("не Float".into())),
    }
}

fn const_double(pool: &ConstantPool<'_>, index: u16) -> Result<f64, ClassfileError> {
    match const_from(pool, index)? {
        Const::Double(v) => Ok(v),
        _ => Err(ClassfileError::Parse("не Double".into())),
    }
}
