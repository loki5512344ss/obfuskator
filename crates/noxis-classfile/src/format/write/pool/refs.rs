//! Constant-pool references: `Fieldref`/`Methodref`/`MethodHandle`/`Dynamic`.
//!
//! This is an `impl Pool` from [`super`], split out so the parent file stays
//! within the structure limit. Each builder is memoized by a semantic key
//! (owner/name/descriptor/…) - an identical reference always gets the same index,
//! and the pool doesn't bloat with duplicates (bloat would otherwise widen `ldc`
//! into `ldc_w` and change code length on relocation).

use ristretto_classfile::{Constant, ReferenceKind};

use crate::error::ClassfileError;
use crate::insn::InvokeKind;

use super::{Pool, write_err};

impl Pool {
    pub(crate) fn field_ref(
        &mut self,
        owner: &str,
        name: &str,
        descriptor: &str,
    ) -> Result<u16, ClassfileError> {
        let key = (owner.to_string(), name.to_string(), descriptor.to_string());
        if let Some(&index) = self.field_refs.get(&key) {
            return Ok(index);
        }
        let class_index = self.class(owner)?;
        let name_and_type_index = self.name_and_type(name, descriptor)?;
        let index = self
            .pool
            .add(Constant::FieldRef {
                class_index,
                name_and_type_index,
            })
            .map_err(write_err)?;
        self.field_refs.insert(key, index);
        Ok(index)
    }

    pub(crate) fn method_ref(
        &mut self,
        owner: &str,
        name: &str,
        descriptor: &str,
        kind: InvokeKind,
    ) -> Result<u16, ClassfileError> {
        let itf = matches!(kind, InvokeKind::Interface);
        let key = (
            owner.to_string(),
            name.to_string(),
            descriptor.to_string(),
            itf,
        );
        if let Some(&index) = self.method_refs.get(&key) {
            return Ok(index);
        }
        let class_index = self.class(owner)?;
        let name_and_type_index = self.name_and_type(name, descriptor)?;
        let constant = if itf {
            Constant::InterfaceMethodRef {
                class_index,
                name_and_type_index,
            }
        } else {
            Constant::MethodRef {
                class_index,
                name_and_type_index,
            }
        };
        let index = self.pool.add(constant).map_err(write_err)?;
        self.method_refs.insert(key, index);
        Ok(index)
    }

    pub(crate) fn method_handle(
        &mut self,
        kind: u8,
        owner: &str,
        name: &str,
        descriptor: &str,
        itf: bool,
    ) -> Result<u16, ClassfileError> {
        let key = (
            kind,
            owner.to_string(),
            name.to_string(),
            descriptor.to_string(),
            itf,
        );
        if let Some(&index) = self.handles.get(&key) {
            return Ok(index);
        }
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
        let index = self
            .pool
            .add_method_handle(reference_kind, reference)
            .map_err(write_err)?;
        self.handles.insert(key, index);
        Ok(index)
    }

    pub(crate) fn invoke_dynamic(
        &mut self,
        bootstrap: u16,
        name: &str,
        descriptor: &str,
    ) -> Result<u16, ClassfileError> {
        self.dynamic_like(bootstrap, name, descriptor, true)
    }

    pub(crate) fn dynamic(
        &mut self,
        bootstrap: u16,
        name: &str,
        descriptor: &str,
    ) -> Result<u16, ClassfileError> {
        self.dynamic_like(bootstrap, name, descriptor, false)
    }

    fn dynamic_like(
        &mut self,
        bootstrap: u16,
        name: &str,
        descriptor: &str,
        invoke: bool,
    ) -> Result<u16, ClassfileError> {
        let key = (bootstrap, name.to_string(), descriptor.to_string(), invoke);
        if let Some(&index) = self.dynamics.get(&key) {
            return Ok(index);
        }
        let name_and_type_index = self.name_and_type(name, descriptor)?;
        let constant = if invoke {
            Constant::InvokeDynamic {
                bootstrap_method_attr_index: bootstrap,
                name_and_type_index,
            }
        } else {
            Constant::Dynamic {
                bootstrap_method_attr_index: bootstrap,
                name_and_type_index,
            }
        };
        let index = self.pool.add(constant).map_err(write_err)?;
        self.dynamics.insert(key, index);
        Ok(index)
    }
}
