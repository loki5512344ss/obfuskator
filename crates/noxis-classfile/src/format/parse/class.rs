//! Класс целиком: backend `ClassFile` → наш [`ClassFile`](crate::model::ClassFile).

use ristretto_classfile::attributes::Attribute as RtAttr;
use ristretto_classfile::{
    ClassFile as RtClass, ConstantPool, Field as RtField, Method as RtMethod,
};

use super::attrs;
use super::code::convert_code;
use super::pool::{class_name, utf8};
use crate::error::ClassfileError;
use crate::model::{Attribute, ClassFile, Code, Field, Method, Version};

/// Разобрать байты `.class` в наш IR.
pub(crate) fn parse_class(bytes: &[u8]) -> Result<ClassFile, ClassfileError> {
    let rt = RtClass::from_bytes(bytes).map_err(|e| ClassfileError::Parse(e.to_string()))?;
    let pool = &rt.constant_pool;
    let interfaces = rt
        .interfaces
        .iter()
        .map(|index| class_name(pool, *index))
        .collect::<Result<_, _>>()?;
    Ok(ClassFile {
        version: Version {
            minor: rt.version.minor(),
            major: rt.version.major(),
        },
        access_flags: rt.access_flags.bits(),
        this_class: class_name(pool, rt.this_class)?,
        super_class: optional_class(pool, rt.super_class)?,
        interfaces,
        fields: rt
            .fields
            .iter()
            .map(|field| convert_field(field, pool))
            .collect::<Result<_, _>>()?,
        methods: rt
            .methods
            .iter()
            .map(|method| convert_method(method, pool))
            .collect::<Result<_, _>>()?,
        attributes: convert_attrs(&rt.attributes, pool)?,
    })
}

fn convert_field(field: &RtField, pool: &ConstantPool<'_>) -> Result<Field, ClassfileError> {
    Ok(Field {
        access_flags: field.access_flags.bits(),
        name: utf8(pool, field.name_index)?,
        descriptor: utf8(pool, field.descriptor_index)?,
        attributes: convert_attrs(&field.attributes, pool)?,
    })
}

fn convert_method(method: &RtMethod, pool: &ConstantPool<'_>) -> Result<Method, ClassfileError> {
    let mut code: Option<Code> = None;
    let mut rest: Vec<Attribute> = Vec::new();
    for attr in &method.attributes {
        match attr {
            RtAttr::Code {
                max_stack,
                max_locals,
                code: insns,
                exception_table,
                attributes,
                ..
            } => {
                if code.is_some() {
                    return Err(ClassfileError::Parse("два Code в одном методе".into()));
                }
                code = Some(convert_code(
                    *max_stack,
                    *max_locals,
                    insns,
                    exception_table,
                    attributes,
                    pool,
                )?);
            }
            other => rest.push(attrs::convert(other, pool)?),
        }
    }
    Ok(Method {
        access_flags: method.access_flags.bits(),
        name: utf8(pool, method.name_index)?,
        descriptor: utf8(pool, method.descriptor_index)?,
        code,
        attributes: rest,
    })
}

fn convert_attrs(
    attrs: &[RtAttr],
    pool: &ConstantPool<'_>,
) -> Result<Vec<Attribute>, ClassfileError> {
    attrs
        .iter()
        .map(|attr| attrs::convert(attr, pool))
        .collect()
}

/// Индекс суперкласса, где `0` — его отсутствие (`java/lang/Object`).
fn optional_class(pool: &ConstantPool<'_>, index: u16) -> Result<Option<String>, ClassfileError> {
    if index == 0 {
        Ok(None)
    } else {
        Ok(Some(class_name(pool, index)?))
    }
}
