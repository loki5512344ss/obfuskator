//! Annotation attributes → IR: names instead of pool indices.
//!
//! Split out of `pool.rs`: these structures carry pool references that go
//! stale the moment the writer rebuilds the pool, so every one of them is
//! modeled (raw bytes made ASM die with `ArrayIndexOutOfBoundsException`).

use ristretto_classfile::ConstantPool;

use super::pool::{const_from, utf8};
use crate::error::ClassfileError;
use crate::insn::Const;
use crate::model::{Annotation, AnnotationValue, TypeAnnotation};

/// A list of `annotation` structures (`RuntimeVisibleAnnotations`).
pub(super) fn annos(
    pool: &ConstantPool<'_>,
    list: &[ristretto_classfile::attributes::Annotation],
) -> Result<Vec<Annotation>, ClassfileError> {
    list.iter().map(|anno| annotation(pool, anno)).collect()
}

/// Per-parameter annotation lists (`RuntimeVisibleParameterAnnotations`).
pub(super) fn params(
    pool: &ConstantPool<'_>,
    list: &[ristretto_classfile::attributes::ParameterAnnotation],
) -> Result<Vec<Vec<Annotation>>, ClassfileError> {
    list.iter()
        .map(|param| {
            param
                .annotations
                .iter()
                .map(|anno| annotation(pool, anno))
                .collect()
        })
        .collect()
}

/// A list of `type_annotation` structures (`RuntimeVisibleTypeAnnotations`).
pub(super) fn type_annos(
    pool: &ConstantPool<'_>,
    list: &[ristretto_classfile::attributes::TypeAnnotation],
) -> Result<Vec<TypeAnnotation>, ClassfileError> {
    list.iter()
        .map(|anno| type_annotation(pool, anno))
        .collect()
}

/// One `element_value` → IR, keeping the tag (HotSpot checks it).
pub(super) fn element_value(
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
                .map(|value| element_value(pool, value))
                .collect::<Result<_, _>>()?,
        ),
    })
}

/// One `annotation` structure → IR (names, not pool indices).
fn annotation(
    pool: &ConstantPool<'_>,
    anno: &ristretto_classfile::attributes::Annotation,
) -> Result<Annotation, ClassfileError> {
    Ok(Annotation {
        descriptor: utf8(pool, anno.type_index)?,
        elements: anno
            .elements
            .iter()
            .map(|pair| {
                Ok((
                    utf8(pool, pair.name_index)?,
                    element_value(pool, &pair.value)?,
                ))
            })
            .collect::<Result<_, ClassfileError>>()?,
    })
}

/// One `type_annotation` structure → IR: the annotated type by name,
/// `target_info` + `type_path` as raw bytes (JVMS §4.7.20.1-2 carry no
/// pool references, so they survive the rebuild verbatim).
fn type_annotation(
    pool: &ConstantPool<'_>,
    anno: &ristretto_classfile::attributes::TypeAnnotation,
) -> Result<TypeAnnotation, ClassfileError> {
    let mut target = Vec::new();
    anno.target_type
        .to_bytes(&mut target)
        .map_err(|e| ClassfileError::Parse(e.to_string()))?;
    let path_len = u8::try_from(anno.type_path.len())
        .map_err(|_| ClassfileError::Parse("слишком длинный type_path".into()))?;
    target.push(path_len);
    for path in &anno.type_path {
        path.to_bytes(&mut target)
            .map_err(|e| ClassfileError::Parse(e.to_string()))?;
    }
    Ok(TypeAnnotation {
        target,
        descriptor: utf8(pool, anno.type_index)?,
        elements: anno
            .elements
            .iter()
            .map(|pair| {
                Ok((
                    utf8(pool, pair.name_index)?,
                    element_value(pool, &pair.value)?,
                ))
            })
            .collect::<Result<_, ClassfileError>>()?,
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
