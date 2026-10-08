//! Атрибуты backend'а → атрибуты нашего IR.
//!
//! `BootstrapMethods`, `EnclosingMethod`, `MethodParameters` и `ConstantValue`
//! моделируются явно (в них лежат ссылки пула, переживающие пересборку);
//! остальное структурированное, но нам ненужное, уезжает в `Raw` как есть.
//! `Code` и `StackMapTable` здесь не обрабатываются — их разбирает вызывающий
//! код метода (см. `super::class`).

use ristretto_classfile::ConstantPool;
use ristretto_classfile::attributes::Attribute as RtAttr;

use super::pool::{
    class_name, const_from, is_interface, kind_code, name_and_type, ref_parts, utf8,
};
use crate::error::ClassfileError;
use crate::model::{Attribute, BootstrapMethod, InnerClass, MethodParameter, RecordComponent};

/// Один атрибут backend'а → наш атрибут (кроме `Code`/`StackMapTable`).
pub(super) fn convert(attr: &RtAttr, pool: &ConstantPool<'_>) -> Result<Attribute, ClassfileError> {
    Ok(match attr {
        RtAttr::SourceFile {
            source_file_index, ..
        } => Attribute::SourceFile(utf8(pool, *source_file_index)?),
        RtAttr::Signature {
            signature_index, ..
        } => Attribute::Signature(utf8(pool, *signature_index)?),
        RtAttr::Exceptions {
            exception_indexes, ..
        } => Attribute::Exceptions(
            exception_indexes
                .iter()
                .map(|index| class_name(pool, *index))
                .collect::<Result<_, _>>()?,
        ),
        RtAttr::InnerClasses { classes, .. } => Attribute::InnerClasses(
            classes
                .iter()
                .map(|class| inner(pool, class))
                .collect::<Result<_, _>>()?,
        ),
        RtAttr::NestHost {
            host_class_index, ..
        } => Attribute::NestHost(class_name(pool, *host_class_index)?),
        RtAttr::NestMembers { class_indexes, .. } => Attribute::NestMembers(
            class_indexes
                .iter()
                .map(|index| class_name(pool, *index))
                .collect::<Result<_, _>>()?,
        ),
        RtAttr::EnclosingMethod {
            class_index,
            method_index,
            ..
        } => Attribute::EnclosingMethod {
            class: class_name(pool, *class_index)?,
            method: enclosing_method(pool, *method_index)?,
        },
        RtAttr::MethodParameters { parameters, .. } => Attribute::MethodParameters(
            parameters
                .iter()
                .map(|param| method_param(pool, param))
                .collect::<Result<_, _>>()?,
        ),
        RtAttr::BootstrapMethods { methods, .. } => Attribute::BootstrapMethods(
            methods
                .iter()
                .map(|method| bootstrap(pool, method))
                .collect::<Result<_, _>>()?,
        ),
        RtAttr::ConstantValue {
            constant_value_index,
            ..
        } => Attribute::ConstantValue(const_from(pool, *constant_value_index)?),
        RtAttr::Deprecated { .. } => Attribute::Deprecated,
        RtAttr::Synthetic { .. } => Attribute::Synthetic,
        RtAttr::LineNumberTable { .. }
        | RtAttr::LocalVariableTable { .. }
        | RtAttr::LocalVariableTypeTable { .. } => {
            return Err(ClassfileError::Parse(
                "таблицы кода разбирает вызывающий код метода".into(),
            ));
        }
        RtAttr::Record { records, .. } => Attribute::Record(
            records
                .iter()
                .map(|record| {
                    Ok(RecordComponent {
                        name: utf8(pool, record.name_index)?,
                        descriptor: utf8(pool, record.descriptor_index)?,
                        attributes: record
                            .attributes
                            .iter()
                            .map(|attr| convert(attr, pool))
                            .collect::<Result<_, _>>()?,
                    })
                })
                .collect::<Result<_, ClassfileError>>()?,
        ),
        RtAttr::Code { .. } | RtAttr::StackMapTable { .. } => {
            return Err(ClassfileError::Parse(
                "Code/StackMapTable разбирает вызывающий код метода".into(),
            ));
        }
        other => raw(other)?,
    })
}

/// Несмоделированный атрибут → `Raw`: имя + байты `info` (без заголовка).
pub(super) fn raw(attr: &RtAttr) -> Result<Attribute, ClassfileError> {
    let mut bytes = Vec::new();
    attr.to_bytes(&mut bytes)
        .map_err(|e| ClassfileError::Parse(e.to_string()))?;
    let info = bytes
        .get(6..)
        .ok_or_else(|| ClassfileError::Parse("короткая запись атрибута".into()))?;
    Ok(Attribute::Raw {
        name: attr.name().to_string(),
        data: info.to_vec(),
    })
}

fn inner(
    pool: &ConstantPool<'_>,
    class: &ristretto_classfile::attributes::InnerClass,
) -> Result<InnerClass, ClassfileError> {
    Ok(InnerClass {
        inner: class_name(pool, class.class_info_index)?,
        outer: optional_class(pool, class.outer_class_info_index)?,
        inner_name: optional_utf8(pool, class.name_index)?,
        access_flags: class.access_flags.bits(),
    })
}

/// Метод объемлющего класса (`0` — только класс, без метода).
fn enclosing_method(
    pool: &ConstantPool<'_>,
    index: u16,
) -> Result<Option<(String, String)>, ClassfileError> {
    if index == 0 {
        Ok(None)
    } else {
        Ok(Some(name_and_type(pool, index)?))
    }
}

fn method_param(
    pool: &ConstantPool<'_>,
    param: &ristretto_classfile::attributes::MethodParameter,
) -> Result<MethodParameter, ClassfileError> {
    Ok(MethodParameter {
        name: optional_utf8(pool, param.name_index)?,
        access_flags: param.access_flags.bits(),
    })
}

fn bootstrap(
    pool: &ConstantPool<'_>,
    method: &ristretto_classfile::attributes::BootstrapMethod,
) -> Result<BootstrapMethod, ClassfileError> {
    let constant = pool
        .try_get(method.bootstrap_method_ref)
        .map_err(|e| ClassfileError::Parse(e.to_string()))?;
    let (kind, owner, name, descriptor, itf) = match constant {
        ristretto_classfile::Constant::MethodHandle {
            reference_kind,
            reference_index,
        } => {
            let (owner, name, descriptor) = ref_parts(pool, *reference_index)?;
            (
                kind_code(reference_kind),
                owner,
                name,
                descriptor,
                is_interface(pool, *reference_index),
            )
        }
        other => {
            return Err(ClassfileError::Parse(format!(
                "bootstrap-ссылка обязана быть MethodHandle, найден {other:?}"
            )));
        }
    };
    Ok(BootstrapMethod {
        kind,
        owner,
        name,
        descriptor,
        itf,
        args: method
            .arguments
            .iter()
            .map(|index| const_from(pool, *index))
            .collect::<Result<_, _>>()?,
    })
}

/// Индекс пула, где `0` означает отсутствие значения.
fn optional_class(pool: &ConstantPool<'_>, index: u16) -> Result<Option<String>, ClassfileError> {
    if index == 0 {
        Ok(None)
    } else {
        Ok(Some(class_name(pool, index)?))
    }
}

/// Индекс `CONSTANT_Utf8`, где `0` означает отсутствие значения.
fn optional_utf8(pool: &ConstantPool<'_>, index: u16) -> Result<Option<String>, ClassfileError> {
    if index == 0 {
        Ok(None)
    } else {
        Ok(Some(utf8(pool, index)?))
    }
}
