//! Our IR → `.class` bytes through the `ristretto_classfile` backend.
//!
//! The pool is built from scratch by a deterministic walk; the backend recomputes
//! instruction numbers into byte offsets and verifies the result (`verify`).

mod annotations;
mod attrs;
pub(crate) mod code;
mod helpers;
pub(crate) mod pool;

use ristretto_classfile::{
    ClassAccessFlags, ClassFile as RtClass, Field as RtField, FieldAccessFlags, FieldType,
    Method as RtMethod, MethodAccessFlags, Version as RtVersion,
};

use crate::error::ClassfileError;
use crate::model::{ClassFile, Field, Method};

/// Our IR class → `.class` bytes.
pub(crate) fn write_class(class: &ClassFile) -> Result<Vec<u8>, ClassfileError> {
    let mut pool = pool::Pool::new();
    let version = RtVersion::from(class.version.major, class.version.minor)
        .map_err(|e| ClassfileError::Write(e.to_string()))?;
    let this_class = pool.class(&class.this_class)?;
    let super_class = match &class.super_class {
        None => 0,
        Some(name) => pool.class(name)?,
    };
    let mut interfaces = Vec::with_capacity(class.interfaces.len());
    for interface in &class.interfaces {
        interfaces.push(pool.class(interface)?);
    }
    let mut fields = Vec::with_capacity(class.fields.len());
    for field in &class.fields {
        fields.push(convert_field(field, &mut pool)?);
    }
    let mut methods = Vec::with_capacity(class.methods.len());
    for method in &class.methods {
        methods.push(convert_method(method, &mut pool)?);
    }
    let mut attributes = Vec::with_capacity(class.attributes.len());
    for attr in &class.attributes {
        attributes.push(attrs::convert(attr, &mut pool)?);
    }
    let rt = RtClass {
        version,
        constant_pool: pool.finish(),
        access_flags: ClassAccessFlags::from_bits_truncate(class.access_flags),
        this_class,
        super_class,
        interfaces,
        fields,
        methods,
        attributes,
        code_source_url: None,
    };
    rt.verify()
        .map_err(|e| ClassfileError::Write(format!("собранный класс не верифицируется: {e}")))?;
    let mut bytes = Vec::new();
    rt.to_bytes(&mut bytes)
        .map_err(|e| ClassfileError::Write(e.to_string()))?;
    Ok(bytes)
}

fn convert_field(field: &Field, pool: &mut pool::Pool) -> Result<RtField, ClassfileError> {
    Ok(RtField {
        access_flags: FieldAccessFlags::from_bits_truncate(field.access_flags),
        name_index: pool.utf8(&field.name)?,
        descriptor_index: pool.utf8(&field.descriptor)?,
        field_type: FieldType::parse(&field.descriptor)
            .map_err(|e| ClassfileError::Write(format!("кривой дескриптор поля: {e}")))?,
        attributes: field
            .attributes
            .iter()
            .map(|attr| attrs::convert(attr, pool))
            .collect::<Result<_, _>>()?,
    })
}

fn convert_method(method: &Method, pool: &mut pool::Pool) -> Result<RtMethod, ClassfileError> {
    let mut attributes = Vec::with_capacity(method.attributes.len() + 1);
    for attr in &method.attributes {
        attributes.push(attrs::convert(attr, pool)?);
    }
    if let Some(code) = &method.code {
        attributes.push(code::convert_code(code, pool)?);
    }
    Ok(RtMethod {
        access_flags: MethodAccessFlags::from_bits_truncate(method.access_flags),
        name_index: pool.utf8(&method.name)?,
        descriptor_index: pool.utf8(&method.descriptor)?,
        attributes,
    })
}
