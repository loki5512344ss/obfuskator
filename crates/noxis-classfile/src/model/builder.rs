//! Сборка классов «с нуля»: junk-классы, рантайм-декодеры, аннотации-шум.

use crate::model::{Attribute, ClassFile, Code, Field, Method, Version};

/// Строитель класса без бинарного представления на входе.
#[derive(Debug, Clone)]
pub struct ClassBuilder {
    version: Version,
    access_flags: u16,
    this_class: String,
    super_class: Option<String>,
    interfaces: Vec<String>,
    fields: Vec<Field>,
    methods: Vec<Method>,
    attributes: Vec<Attribute>,
}

impl ClassBuilder {
    /// Новый класс `public` с указанным суперклассом (`None` = `java/lang/Object`,
    /// кроме самого `java/lang/Object` — у него суперкласса нет).
    pub fn new(this_class: impl Into<String>, super_class: Option<&str>) -> Self {
        let this_class = this_class.into();
        let super_class = match super_class {
            Some(name) => Some(name.to_string()),
            None if this_class == "java/lang/Object" => None,
            None => Some("java/lang/Object".to_string()),
        };
        Self {
            version: Version::JAVA_21,
            access_flags: 0x0021, // ACC_PUBLIC | ACC_SUPER
            this_class,
            super_class,
            interfaces: Vec::new(),
            fields: Vec::new(),
            methods: Vec::new(),
            attributes: Vec::new(),
        }
    }

    /// Задать версию классового файла.
    pub fn version(mut self, version: Version) -> Self {
        self.version = version;
        self
    }

    /// Задать флаги доступа (перезаписывает `ACC_PUBLIC|ACC_SUPER`).
    pub fn access_flags(mut self, flags: u16) -> Self {
        self.access_flags = flags;
        self
    }

    /// Добавить интерфейс.
    pub fn interface(mut self, name: impl Into<String>) -> Self {
        self.interfaces.push(name.into());
        self
    }

    /// Добавить поле.
    pub fn field(mut self, name: impl Into<String>, descriptor: impl Into<String>) -> Self {
        self.fields.push(Field {
            access_flags: 0x0001, // ACC_PUBLIC
            name: name.into(),
            descriptor: descriptor.into(),
            attributes: Vec::new(),
        });
        self
    }

    /// Добавить метод без тела (`abstract`/`native` должны выставить флаги сами).
    pub fn method(
        mut self,
        access_flags: u16,
        name: impl Into<String>,
        descriptor: impl Into<String>,
    ) -> Self {
        self.methods.push(Method {
            access_flags,
            name: name.into(),
            descriptor: descriptor.into(),
            code: None,
            attributes: Vec::new(),
        });
        self
    }

    /// Добавить метод с телом.
    pub fn method_with_code(
        mut self,
        access_flags: u16,
        name: impl Into<String>,
        descriptor: impl Into<String>,
        code: Code,
    ) -> Self {
        self.methods.push(Method {
            access_flags,
            name: name.into(),
            descriptor: descriptor.into(),
            code: Some(code),
            attributes: Vec::new(),
        });
        self
    }

    /// Добавить атрибут уровня класса.
    pub fn attribute(mut self, attribute: Attribute) -> Self {
        self.attributes.push(attribute);
        self
    }

    /// Собрать класс.
    pub fn build(self) -> ClassFile {
        ClassFile {
            version: self.version,
            access_flags: self.access_flags,
            this_class: self.this_class,
            super_class: self.super_class,
            interfaces: self.interfaces,
            fields: self.fields,
            methods: self.methods,
            attributes: self.attributes,
        }
    }
}

/// Пустое тело метода (`return`), для junk-методов.
pub fn empty_code(max_locals: u16) -> Code {
    Code {
        max_stack: 0,
        max_locals,
        insns: vec![crate::insn::Insn::Return(None)],
        exception_table: Vec::new(),
        frames: Vec::new(),
        attributes: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]
    use super::*;

    #[test]
    fn builds_class_with_method() {
        let class = ClassBuilder::new("pkg/A", None)
            .field("f", "I")
            .method_with_code(0x0009, "m", "()V", empty_code(0))
            .build();
        assert_eq!(class.this_class, "pkg/A");
        assert_eq!(class.fields.len(), 1);
        assert!(class.method("m", "()V").unwrap().has_code());
    }
}
