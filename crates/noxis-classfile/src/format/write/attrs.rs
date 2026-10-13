//! Our IR attributes → backend attributes.
//!
//! Attribute names (`SourceFile`, …) are interned into the pool right here - the
//! backend stores only `name_index`, and the string is recovered on read.

use ristretto_classfile::attributes::{
    Attribute as RtAttr, LineNumber as RtLine, LocalVariableTable as RtLocal,
    LocalVariableTypeTable as RtLocalType, Record as RtRecord,
};

use super::annotations;
use super::helpers;
use super::pool::Pool;
use crate::error::ClassfileError;
use crate::insn::Label;
use crate::model::{Attribute, LineNumber, LocalVariable, LocalVariableType};

/// Code-table positions: labels → bytes of the current layout.
pub(super) struct Positions<'a> {
    pub resolve: &'a dyn Fn(Label) -> Result<u32, ClassfileError>,
    pub byte_of: &'a dyn Fn(u32) -> Result<u16, ClassfileError>,
}

/// `LineNumberTable`: the backend stores instruction numbers and maps them itself.
pub(super) fn line_number_table(
    lines: &[LineNumber],
    pool: &mut Pool,
    positions: &Positions<'_>,
) -> Result<RtAttr, ClassfileError> {
    Ok(RtAttr::LineNumberTable {
        name_index: pool.utf8("LineNumberTable")?,
        line_numbers: lines
            .iter()
            .map(|line| {
                let index = (positions.resolve)(line.start)?;
                Ok(RtLine {
                    start_pc: u16::try_from(index).map_err(|_| {
                        ClassfileError::Write(format!("номер инструкции {index} вне u16"))
                    })?,
                    line_number: line.line,
                })
            })
            .collect::<Result<_, ClassfileError>>()?,
    })
}

/// `LocalVariableTable`/`TypeTable` with the range recomputed.
pub(super) fn local_table(
    vars: &[LocalVariable],
    pool: &mut Pool,
    positions: &Positions<'_>,
) -> Result<RtAttr, ClassfileError> {
    Ok(RtAttr::LocalVariableTable {
        name_index: pool.utf8("LocalVariableTable")?,
        variables: vars
            .iter()
            .map(|var| {
                let (start_pc, length) = range(var.start, var.end, positions)?;
                Ok(RtLocal {
                    start_pc,
                    length,
                    name_index: pool.utf8(&var.name)?,
                    descriptor_index: pool.utf8(&var.descriptor)?,
                    index: var.index,
                })
            })
            .collect::<Result<_, ClassfileError>>()?,
    })
}
/// The same table for types (`signature` instead of a descriptor).
pub(super) fn local_type_table(
    vars: &[LocalVariableType],
    pool: &mut Pool,
    positions: &Positions<'_>,
) -> Result<RtAttr, ClassfileError> {
    Ok(RtAttr::LocalVariableTypeTable {
        name_index: pool.utf8("LocalVariableTypeTable")?,
        variable_types: vars
            .iter()
            .map(|var| {
                let (start_pc, length) = range(var.start, var.end, positions)?;
                Ok(RtLocalType {
                    start_pc,
                    length,
                    name_index: pool.utf8(&var.name)?,
                    signature_index: pool.utf8(&var.signature)?,
                    index: var.index,
                })
            })
            .collect::<Result<_, ClassfileError>>()?,
    })
}

fn range(
    start: Label,
    end: Label,
    positions: &Positions<'_>,
) -> Result<(u16, u16), ClassfileError> {
    let first = (positions.byte_of)((positions.resolve)(start)?)?;
    let last = (positions.byte_of)((positions.resolve)(end)?)?;
    let length = last
        .checked_sub(first)
        .ok_or_else(|| ClassfileError::Write("конец раньше начала".into()))?;
    Ok((first, length))
}

/// One of our attributes → a backend attribute.
pub(super) fn convert(attr: &Attribute, pool: &mut Pool) -> Result<RtAttr, ClassfileError> {
    Ok(match attr {
        Attribute::SourceFile(name) => RtAttr::SourceFile {
            name_index: pool.utf8("SourceFile")?,
            source_file_index: pool.utf8(name)?,
        },
        Attribute::Signature(value) => RtAttr::Signature {
            name_index: pool.utf8("Signature")?,
            signature_index: pool.utf8(value)?,
        },
        Attribute::Exceptions(names) => RtAttr::Exceptions {
            name_index: pool.utf8("Exceptions")?,
            exception_indexes: names
                .iter()
                .map(|name| pool.class(name))
                .collect::<Result<_, _>>()?,
        },
        Attribute::Deprecated => RtAttr::Deprecated {
            name_index: pool.utf8("Deprecated")?,
        },
        Attribute::Synthetic => RtAttr::Synthetic {
            name_index: pool.utf8("Synthetic")?,
        },
        Attribute::InnerClasses(classes) => RtAttr::InnerClasses {
            name_index: pool.utf8("InnerClasses")?,
            classes: classes
                .iter()
                .map(|class| helpers::inner(class, pool))
                .collect::<Result<_, _>>()?,
        },
        Attribute::NestHost(host) => RtAttr::NestHost {
            name_index: pool.utf8("NestHost")?,
            host_class_index: pool.class(host)?,
        },
        Attribute::NestMembers(members) => RtAttr::NestMembers {
            name_index: pool.utf8("NestMembers")?,
            class_indexes: members
                .iter()
                .map(|name| pool.class(name))
                .collect::<Result<_, _>>()?,
        },
        Attribute::PermittedSubclasses(classes) => RtAttr::PermittedSubclasses {
            name_index: pool.utf8("PermittedSubclasses")?,
            class_indexes: classes
                .iter()
                .map(|name| pool.class(name))
                .collect::<Result<_, _>>()?,
        },
        Attribute::ConstantValue(value) => RtAttr::ConstantValue {
            name_index: pool.utf8("ConstantValue")?,
            constant_value_index: pool.constant(value)?,
        },
        Attribute::BootstrapMethods(methods) => RtAttr::BootstrapMethods {
            name_index: pool.utf8("BootstrapMethods")?,
            methods: methods
                .iter()
                .map(|method| helpers::bootstrap(method, pool))
                .collect::<Result<_, _>>()?,
        },
        Attribute::EnclosingMethod { class, method } => RtAttr::EnclosingMethod {
            name_index: pool.utf8("EnclosingMethod")?,
            class_index: pool.class(class)?,
            method_index: match method {
                None => 0,
                Some((name, descriptor)) => pool.name_and_type(name, descriptor)?,
            },
        },
        Attribute::MethodParameters(params) => RtAttr::MethodParameters {
            name_index: pool.utf8("MethodParameters")?,
            parameters: params
                .iter()
                .map(|param| helpers::method_param(param, pool))
                .collect::<Result<_, _>>()?,
        },
        Attribute::RuntimeVisibleAnnotations(annos) => RtAttr::RuntimeVisibleAnnotations {
            name_index: pool.utf8("RuntimeVisibleAnnotations")?,
            annotations: annotations::list(annos, pool)?,
        },
        Attribute::RuntimeInvisibleAnnotations(annos) => RtAttr::RuntimeInvisibleAnnotations {
            name_index: pool.utf8("RuntimeInvisibleAnnotations")?,
            annotations: annotations::list(annos, pool)?,
        },
        Attribute::RuntimeVisibleParameterAnnotations(params) => {
            RtAttr::RuntimeVisibleParameterAnnotations {
                name_index: pool.utf8("RuntimeVisibleParameterAnnotations")?,
                parameter_annotations: annotations::parameters(params, pool)?,
            }
        }
        Attribute::RuntimeInvisibleParameterAnnotations(params) => {
            RtAttr::RuntimeInvisibleParameterAnnotations {
                name_index: pool.utf8("RuntimeInvisibleParameterAnnotations")?,
                parameter_annotations: annotations::parameters(params, pool)?,
            }
        }
        Attribute::RuntimeVisibleTypeAnnotations(annos) => RtAttr::RuntimeVisibleTypeAnnotations {
            name_index: pool.utf8("RuntimeVisibleTypeAnnotations")?,
            type_annotations: annotations::type_list(annos, pool)?,
        },
        Attribute::RuntimeInvisibleTypeAnnotations(annos) => {
            RtAttr::RuntimeInvisibleTypeAnnotations {
                name_index: pool.utf8("RuntimeInvisibleTypeAnnotations")?,
                type_annotations: annotations::type_list(annos, pool)?,
            }
        }
        Attribute::AnnotationDefault(value) => RtAttr::AnnotationDefault {
            name_index: pool.utf8("AnnotationDefault")?,
            element: annotations::element(value, pool)?,
        },
        Attribute::Raw { name, data } => RtAttr::Unknown {
            name_index: pool.utf8(name)?,
            info: data.clone(),
        },
        Attribute::LineNumberTable(_)
        | Attribute::LocalVariableTable(_)
        | Attribute::LocalVariableTypeTable(_) => {
            return Err(ClassfileError::Write(
                "таблицы кода пишет вызывающий код метода".into(),
            ));
        }
        Attribute::Record(records) => RtAttr::Record {
            name_index: pool.utf8("Record")?,
            records: records
                .iter()
                .map(|record| {
                    Ok(RtRecord {
                        name_index: pool.utf8(&record.name)?,
                        descriptor_index: pool.utf8(&record.descriptor)?,
                        attributes: record
                            .attributes
                            .iter()
                            .map(|attr| convert(attr, pool))
                            .collect::<Result<_, _>>()?,
                    })
                })
                .collect::<Result<_, ClassfileError>>()?,
        },
    })
}
