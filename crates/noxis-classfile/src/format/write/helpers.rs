//! Хелперы сборки `InnerClasses`/`BootstrapMethods`/`MethodParameters`.
//!
//! Вынесены из `attrs`, чтобы тот держался в лимите 250 строк; логика не
//! изменилась — только место. Каждый строит запись backend'а из элемента IR,
//! интернируя имена/дескрипторы и резолвя индексы `CONSTANT_Class` через пул.

use ristretto_classfile::MethodAccessFlags;
use ristretto_classfile::attributes::{
    BootstrapMethod as RtBsm, InnerClass as RtInner, MethodParameter as RtParam,
    NestedClassAccessFlags,
};

use super::pool::Pool;
use crate::error::ClassfileError;
use crate::insn::Const;
use crate::model::{BootstrapMethod, InnerClass, MethodParameter};

pub(super) fn inner(class: &InnerClass, pool: &mut Pool) -> Result<RtInner, ClassfileError> {
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

pub(super) fn bootstrap(
    method: &BootstrapMethod,
    pool: &mut Pool,
) -> Result<RtBsm, ClassfileError> {
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

pub(super) fn method_param(
    param: &MethodParameter,
    pool: &mut Pool,
) -> Result<RtParam, ClassfileError> {
    Ok(RtParam {
        name_index: match &param.name {
            None => 0,
            Some(name) => pool.utf8(name)?,
        },
        access_flags: MethodAccessFlags::from_bits_truncate(param.access_flags),
    })
}
