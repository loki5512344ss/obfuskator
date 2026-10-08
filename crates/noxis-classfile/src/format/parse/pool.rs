//! Разрешение индексов constant pool в строки нашего IR.

use ristretto_classfile::{Constant, ConstantPool, ReferenceKind};

use crate::error::ClassfileError;
use crate::insn::{Const, InvokeKind};

/// Имя класса по `CONSTANT_Class`.
pub(super) fn class_name(pool: &ConstantPool<'_>, index: u16) -> Result<String, ClassfileError> {
    let name = pool
        .try_get_class(index)
        .map_err(|e| ClassfileError::Parse(e.to_string()))?;
    Ok(name.to_string())
}

/// `(owner, name, descriptor)` для `CONSTANT_Fieldref`/`_Methodref`.
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

/// `(name, descriptor)` для `CONSTANT_NameAndType`.
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

/// Строка из `CONSTANT_Utf8`.
pub(super) fn utf8(pool: &ConstantPool<'_>, index: u16) -> Result<String, ClassfileError> {
    let value = pool
        .try_get_utf8(index)
        .map_err(|e| ClassfileError::Parse(e.to_string()))?;
    Ok(value.to_string())
}

/// Константа для `ldc`/`ldc_w`/`ldc2_w`.
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

/// `(name, descriptor, bootstrap)` для `CONSTANT_InvokeDynamic`.
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

/// Вид вызова по индексу ссылки: интерфейсная или обычная.
pub(super) fn is_interface(pool: &ConstantPool<'_>, index: u16) -> bool {
    matches!(pool.try_get(index), Ok(Constant::InterfaceMethodRef { .. }))
}

/// JVMS §5.4.3.5: номер `reference_kind` для `CONSTANT_MethodHandle`.
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

/// Проверка, что `Invoke` действительно ссылается на ожидаемый вид.
///
/// JVM принимает `Methodref` и `InterfaceMethodref` для всех видов вызова,
/// кроме `invokeinterface` (там только интерфейсная); строгость сверх JVM
/// ломает чтение реальных классов (статические методы интерфейсов).
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
