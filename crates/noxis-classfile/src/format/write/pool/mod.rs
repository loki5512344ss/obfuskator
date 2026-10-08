//! Сборка constant pool при записи: строки нашего IR → индексы backend'а.
//!
//! Пул строится с нуля детерминированным обходом (классы, поля, методы,
//! атрибуты, код), поэтому одинаковый IR даёт одинаковые байты. Индексы
//! `Raw`-атрибутов при этом не переносятся — см. `super::attrs`.
//!
//! Backend не дедуплицирует: `add_*` всегда толкает новый элемент. Кэшируем
//! семантические ключи сами — иначе `LocalVariableTable` и
//! `LocalVariableTypeTable` получат разные `name_index` для одного имени, и
//! HotSpot отвергнет класс (`LVTT entry ... does not match any LVT entry`).
//! Порядок обхода детерминирован, поэтому кэш меняет только компактность пула.
//! Референсы (поле/метод/хендл/динамик) — в [`refs`].

use std::collections::HashMap;

use ristretto_classfile::{Constant, ConstantPool};

use crate::error::ClassfileError;
use crate::insn::Const;

mod refs;

/// Строитель пула; живёт только внутри одного `write`.
pub(crate) struct Pool {
    pub(super) pool: ConstantPool<'static>,
    pub(super) utf8s: HashMap<String, u16>,
    pub(super) classes: HashMap<String, u16>,
    pub(super) name_types: HashMap<(String, String), u16>,
    pub(super) strings: HashMap<String, u16>,
    pub(super) method_types: HashMap<String, u16>,
    pub(super) field_refs: HashMap<(String, String, String), u16>,
    pub(super) method_refs: HashMap<(String, String, String, bool), u16>,
    pub(super) handles: HashMap<(u8, String, String, String, bool), u16>,
    pub(super) dynamics: HashMap<(u16, String, String, bool), u16>,
}

impl Pool {
    pub(crate) fn new() -> Self {
        Pool {
            pool: ConstantPool::default(),
            utf8s: HashMap::new(),
            classes: HashMap::new(),
            name_types: HashMap::new(),
            strings: HashMap::new(),
            method_types: HashMap::new(),
            field_refs: HashMap::new(),
            method_refs: HashMap::new(),
            handles: HashMap::new(),
            dynamics: HashMap::new(),
        }
    }

    pub(super) fn utf8(&mut self, value: &str) -> Result<u16, ClassfileError> {
        if let Some(&index) = self.utf8s.get(value) {
            return Ok(index);
        }
        let index = self.pool.add_utf8(value).map_err(write_err)?;
        self.utf8s.insert(value.to_string(), index);
        Ok(index)
    }

    pub(super) fn name_and_type(
        &mut self,
        name: &str,
        descriptor: &str,
    ) -> Result<u16, ClassfileError> {
        let key = (name.to_string(), descriptor.to_string());
        if let Some(&index) = self.name_types.get(&key) {
            return Ok(index);
        }
        // Собираем из примитивов, а не `add_name_and_type`: последний толкает
        // собственные `Utf8`-строки мимо нашего кэша (дубли в пуле).
        let name_index = self.utf8(name)?;
        let descriptor_index = self.utf8(descriptor)?;
        let index = self
            .pool
            .add(Constant::NameAndType {
                name_index,
                descriptor_index,
            })
            .map_err(write_err)?;
        self.name_types.insert(key, index);
        Ok(index)
    }

    pub(super) fn class(&mut self, name: &str) -> Result<u16, ClassfileError> {
        if let Some(&index) = self.classes.get(name) {
            return Ok(index);
        }
        let name_index = self.utf8(name)?;
        let index = self
            .pool
            .add(Constant::Class(name_index))
            .map_err(write_err)?;
        self.classes.insert(name.to_string(), index);
        Ok(index)
    }

    /// Константа для `ldc`/`ldc2_w`; `dynamic` запрещён (только через bootstrap).
    pub(super) fn constant(&mut self, value: &Const) -> Result<u16, ClassfileError> {
        match value {
            Const::Int(v) => self.pool.add_integer(*v).map_err(write_err),
            Const::Float(v) => self.pool.add_float(*v).map_err(write_err),
            Const::Long(v) => self.pool.add_long(*v).map_err(write_err),
            Const::Double(v) => self.pool.add_double(*v).map_err(write_err),
            Const::String(v) => self.string(v),
            Const::Class(v) => self.class(v),
            Const::MethodType(v) => self.method_type(v),
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

    fn string(&mut self, value: &str) -> Result<u16, ClassfileError> {
        if let Some(&index) = self.strings.get(value) {
            return Ok(index);
        }
        let string_index = self.utf8(value)?;
        let index = self
            .pool
            .add(Constant::String(string_index))
            .map_err(write_err)?;
        self.strings.insert(value.to_string(), index);
        Ok(index)
    }

    fn method_type(&mut self, descriptor: &str) -> Result<u16, ClassfileError> {
        if let Some(&index) = self.method_types.get(descriptor) {
            return Ok(index);
        }
        let descriptor_index = self.utf8(descriptor)?;
        let index = self
            .pool
            .add(Constant::MethodType(descriptor_index))
            .map_err(write_err)?;
        self.method_types.insert(descriptor.to_string(), index);
        Ok(index)
    }

    pub(super) fn finish(self) -> ConstantPool<'static> {
        self.pool
    }
}

fn write_err(error: ristretto_classfile::Error) -> ClassfileError {
    ClassfileError::Write(error.to_string())
}
