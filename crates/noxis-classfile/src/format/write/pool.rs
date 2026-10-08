//! Сборка constant pool при записи: строки нашего IR → индексы backend'а.
//!
//! Пул строится с нуля детерминированным обходом (классы, поля, методы,
//! атрибуты, код), поэтому одинаковый IR даёт одинаковые байты. Индексы
//! `Raw`-атрибутов при этом не переносятся — см. `super::attrs`.

use ristretto_classfile::{ConstantPool, ReferenceKind};

use crate::error::ClassfileError;
use crate::insn::{Const, InvokeKind};

/// Строитель пула; живёт только внутри одного `write`.
pub(crate) struct Pool {
    pool: ConstantPool<'static>,
}

impl Pool {
    pub(crate) fn new() -> Self {
        Pool {
            pool: ConstantPool::default(),
        }
    }

    pub(super) fn utf8(&mut self, value: &str) -> Result<u16, ClassfileError> {
        self.pool.add_utf8(value).map_err(write_err)
    }

    pub(super) fn name_and_type(
        &mut self,
        name: &str,
        descriptor: &str,
    ) -> Result<u16, ClassfileError> {
        self.pool
            .add_name_and_type(name, descriptor)
            .map_err(write_err)
    }

    pub(super) fn class(&mut self, name: &str) -> Result<u16, ClassfileError> {
        self.pool.add_class(name).map_err(write_err)
    }

    pub(super) fn field_ref(
        &mut self,
        owner: &str,
        name: &str,
        descriptor: &str,
    ) -> Result<u16, ClassfileError> {
        let owner = self.class(owner)?;
        self.pool
            .add_field_ref(owner, name, descriptor)
            .map_err(write_err)
    }

    pub(super) fn method_ref(
        &mut self,
        owner: &str,
        name: &str,
        descriptor: &str,
        kind: InvokeKind,
    ) -> Result<u16, ClassfileError> {
        let owner = self.class(owner)?;
        match kind {
            InvokeKind::Interface => self
                .pool
                .add_interface_method_ref(owner, name, descriptor)
                .map_err(write_err),
            _ => self
                .pool
                .add_method_ref(owner, name, descriptor)
                .map_err(write_err),
        }
    }

    /// Константа для `ldc`/`ldc2_w`; `dynamic` запрещён (только через bootstrap).
    pub(super) fn constant(&mut self, value: &Const) -> Result<u16, ClassfileError> {
        match value {
            Const::Int(v) => self.pool.add_integer(*v).map_err(write_err),
            Const::Float(v) => self.pool.add_float(*v).map_err(write_err),
            Const::Long(v) => self.pool.add_long(*v).map_err(write_err),
            Const::Double(v) => self.pool.add_double(*v).map_err(write_err),
            Const::String(v) => self.pool.add_string(v).map_err(write_err),
            Const::Class(v) => self.class(v),
            Const::MethodType(v) => self.pool.add_method_type(v).map_err(write_err),
            Const::MethodHandle {
                kind,
                owner,
                name,
                descriptor,
                itf,
            } => self.method_handle(*kind, owner, name, descriptor, *itf),
            Const::Dynamic { .. } => Err(ClassfileError::Write(
                "CONSTANT_Dynamic пишется только через BootstrapMethods".into(),
            )),
        }
    }

    pub(super) fn method_handle(
        &mut self,
        kind: u8,
        owner: &str,
        name: &str,
        descriptor: &str,
        itf: bool,
    ) -> Result<u16, ClassfileError> {
        let reference_kind = ReferenceKind::try_from(kind).map_err(|e| {
            ClassfileError::Write(format!("некорректный reference_kind {kind}: {e}"))
        })?;
        let reference = if itf {
            self.method_ref(owner, name, descriptor, InvokeKind::Interface)?
        } else if matches!(
            reference_kind,
            ReferenceKind::GetField
                | ReferenceKind::GetStatic
                | ReferenceKind::PutField
                | ReferenceKind::PutStatic
        ) {
            self.field_ref(owner, name, descriptor)?
        } else {
            self.method_ref(owner, name, descriptor, InvokeKind::Static)?
        };
        self.pool
            .add_method_handle(reference_kind, reference)
            .map_err(write_err)
    }

    pub(super) fn invoke_dynamic(
        &mut self,
        bootstrap: u16,
        name: &str,
        descriptor: &str,
    ) -> Result<u16, ClassfileError> {
        self.pool
            .add_invoke_dynamic(bootstrap, name, descriptor)
            .map_err(write_err)
    }

    pub(super) fn dynamic(
        &mut self,
        bootstrap: u16,
        name: &str,
        descriptor: &str,
    ) -> Result<u16, ClassfileError> {
        self.pool
            .add_dynamic(bootstrap, name, descriptor)
            .map_err(write_err)
    }

    pub(super) fn finish(self) -> ConstantPool<'static> {
        self.pool
    }
}

fn write_err(error: ristretto_classfile::Error) -> ClassfileError {
    ClassfileError::Write(error.to_string())
}
