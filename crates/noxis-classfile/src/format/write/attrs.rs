//! Атрибуты нашего IR → атрибуты backend'а.
//!
//! Имена атрибутов (`SourceFile`, …) интернируются в пул здесь же — backend
//! хранит только `name_index`, строка восстанавливается при чтении.

use ristretto_classfile::MethodAccessFlags;
use ristretto_classfile::attributes::{
    Attribute as RtAttr, BootstrapMethod as RtBsm, InnerClass as RtInner, LineNumber as RtLine,
    LocalVariableTable as RtLocal, LocalVariableTypeTable as RtLocalType,
    MethodParameter as RtParam, NestedClassAccessFlags, Record as RtRecord,
};

use super::pool::Pool;
use crate::error::ClassfileError;
use crate::insn::{Const, Label};
use crate::model::{
    Attribute, BootstrapMethod, InnerClass, LineNumber, LocalVariable, LocalVariableType,
    MethodParameter,
};

/// Позиции таблиц кода: метки → байты текущего layout.
pub(super) struct Positions<'a> {
    pub resolve: &'a dyn Fn(Label) -> Result<u32, ClassfileError>,
    pub byte_of: &'a dyn Fn(u32) -> Result<u16, ClassfileError>,
}

/// `LineNumberTable`: backend хранит номера инструкций и маппит сам.
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

/// `LocalVariableTable`/`TypeTable` с пересчётом диапазона.
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
/// Та же таблица для типов (`signature` вместо дескриптора).
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

/// Один наш атрибут → атрибут backend'а.
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
                .map(|class| inner(class, pool))
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
                .map(|method| bootstrap(method, pool))
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
                .map(|param| method_param(param, pool))
                .collect::<Result<_, _>>()?,
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

fn inner(class: &InnerClass, pool: &mut Pool) -> Result<RtInner, ClassfileError> {
    Ok(RtInner {
        class_info_index: pool.class(&class.inner)?,
        outer_class_info_index: match &class.outer {
            None => 0,
            Some(name) => pool.class(name)?,
        },
        name_index: match &class.inner_name {
            None => 0,
            Some(name) => pool.utf8(name)?,
        },
        access_flags: NestedClassAccessFlags::from_bits_truncate(class.access_flags),
    })
}

fn bootstrap(method: &BootstrapMethod, pool: &mut Pool) -> Result<RtBsm, ClassfileError> {
    Ok(RtBsm {
        bootstrap_method_ref: pool.method_handle(
            method.kind,
            &method.owner,
            &method.name,
            &method.descriptor,
            method.itf,
        )?,
        arguments: method
            .args
            .iter()
            .map(|arg| bootstrap_arg(arg, pool))
            .collect::<Result<_, _>>()?,
    })
}

/// Аргумент bootstrap-метода: те же константы, что в `ldc`, плюс `Dynamic`.
fn bootstrap_arg(arg: &Const, pool: &mut Pool) -> Result<u16, ClassfileError> {
    match arg {
        Const::Dynamic {
            name,
            descriptor,
            bootstrap,
        } => pool.dynamic(*bootstrap, name, descriptor),
        _ => pool.constant(arg),
    }
}

fn method_param(param: &MethodParameter, pool: &mut Pool) -> Result<RtParam, ClassfileError> {
    Ok(RtParam {
        name_index: match &param.name {
            None => 0,
            Some(name) => pool.utf8(name)?,
        },
        access_flags: MethodAccessFlags::from_bits_truncate(param.access_flags),
    })
}
