//! Runtime-visible/invisible annotations: IR → backend structures.

use ristretto_classfile::attributes::{
    Annotation as RtAnno, AnnotationElement as RtElem, AnnotationValuePair as RtPair,
};

use super::pool::Pool;
use crate::error::ClassfileError;
use crate::insn::Const;
use crate::model::{Annotation, AnnotationValue};
/// One IR annotation → backend structure (fresh pool indices).
pub(super) fn annotation(anno: &Annotation, pool: &mut Pool) -> Result<RtAnno, ClassfileError> {
    Ok(RtAnno {
        type_index: pool.utf8(&anno.descriptor)?,
        elements: anno
            .elements
            .iter()
            .map(|(name, value)| {
                Ok(RtPair {
                    name_index: pool.utf8(name)?,
                    value: element(value, pool)?,
                })
            })
            .collect::<Result<_, ClassfileError>>()?,
    })
}

/// One IR element value → backend structure, tag preserved.
fn element(value: &AnnotationValue, pool: &mut Pool) -> Result<RtElem, ClassfileError> {
    fn int(v: i32, pool: &mut Pool) -> Result<u16, ClassfileError> {
        pool.constant(&Const::Int(v))
    }
    Ok(match value {
        AnnotationValue::Byte(v) => RtElem::Byte {
            const_value_index: int(*v, pool)?,
        },
        AnnotationValue::Char(v) => RtElem::Char {
            const_value_index: int(*v, pool)?,
        },
        AnnotationValue::Short(v) => RtElem::Short {
            const_value_index: int(*v, pool)?,
        },
        AnnotationValue::Boolean(v) => RtElem::Boolean {
            const_value_index: int(*v, pool)?,
        },
        AnnotationValue::Int(v) => RtElem::Int {
            const_value_index: int(*v, pool)?,
        },
        AnnotationValue::Long(v) => RtElem::Long {
            const_value_index: pool.constant(&Const::Long(*v))?,
        },
        AnnotationValue::Float(v) => RtElem::Float {
            const_value_index: pool.constant(&Const::Float(*v))?,
        },
        AnnotationValue::Double(v) => RtElem::Double {
            const_value_index: pool.constant(&Const::Double(*v))?,
        },
        AnnotationValue::String(v) => RtElem::String {
            const_value_index: pool.utf8(v)?,
        },
        AnnotationValue::Enum { descriptor, name } => RtElem::Enum {
            type_name_index: pool.utf8(descriptor)?,
            const_name_index: pool.utf8(name)?,
        },
        AnnotationValue::Class(desc) => RtElem::Class {
            class_info_index: pool.utf8(desc)?,
        },
        AnnotationValue::Nested(nested) => RtElem::Annotation {
            annotation: annotation(nested, pool)?,
        },
        AnnotationValue::Array(values) => RtElem::Array {
            values: values
                .iter()
                .map(|value| element(value, pool))
                .collect::<Result<_, _>>()?,
        },
    })
}
