//! Building classes "from scratch": junk classes, runtime decoders, noise annotations.

use crate::model::{Attribute, ClassFile, Code, Field, Method, Version};

/// A class builder with no binary representation on input.
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
    /// A new `public` class with the given superclass (`None` = `java/lang/Object`,
    /// except for `java/lang/Object` itself - it has no superclass).
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

    /// Set the class-file version.
    pub fn version(mut self, version: Version) -> Self {
        self.version = version;
        self
    }

    /// Set access flags (overwrites `ACC_PUBLIC|ACC_SUPER`).
    pub fn access_flags(mut self, flags: u16) -> Self {
        self.access_flags = flags;
        self
    }

    /// Add an interface.
    pub fn interface(mut self, name: impl Into<String>) -> Self {
        self.interfaces.push(name.into());
        self
    }

    /// Add a field.
    pub fn field(mut self, name: impl Into<String>, descriptor: impl Into<String>) -> Self {
        self.fields.push(Field {
            access_flags: 0x0001, // ACC_PUBLIC
            name: name.into(),
            descriptor: descriptor.into(),
            attributes: Vec::new(),
        });
        self
    }

    /// Add a method without a body (`abstract`/`native` must set their own flags).
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

    /// Add a method with a body.
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

    /// Add a class-level attribute.
    pub fn attribute(mut self, attribute: Attribute) -> Self {
        self.attributes.push(attribute);
        self
    }

    /// Build the class.
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

/// An empty method body (`return`), for junk methods.
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
