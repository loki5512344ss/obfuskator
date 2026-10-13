//! The "program" structure - the unit of work for the pipeline.

pub mod annotations;
pub mod builder;
pub mod version;

pub use annotations::{Annotation, AnnotationValue, TypeAnnotation};
pub use version::Version;

use crate::format::StackMapFrame;
use crate::insn::{Const, Insn, Label};

/// Unknown/specific attributes are stored as bytes - lossless round-trip.
#[derive(Debug, Clone, PartialEq)]
pub enum Attribute {
    SourceFile(String),
    LineNumberTable(Vec<LineNumber>),
    LocalVariableTable(Vec<LocalVariable>),
    Signature(String),
    Exceptions(Vec<String>),
    Deprecated,
    Synthetic,
    InnerClasses(Vec<InnerClass>),
    /// `NestHost`: the class acting as nest host (JVMS §4.7.28).
    NestHost(String),
    /// `NestMembers`: the member classes of this nest host (JVMS §4.7.29).
    NestMembers(Vec<String>),
    /// `PermittedSubclasses`: who may extend a sealed class (JVMS §4.7.31).
    /// Stored as names: pool indices go stale when the pool is rebuilt.
    PermittedSubclasses(Vec<String>),
    /// A field's `ConstantValue` (`static final`): a value from the pool.
    ConstantValue(Const),
    /// `BootstrapMethods` for `invokedynamic` (lambdas): method + arguments.
    BootstrapMethods(Vec<BootstrapMethod>),
    /// `EnclosingMethod`: the class and optional method enclosing a local class.
    EnclosingMethod {
        class: String,
        method: Option<(String, String)>,
    },
    /// `MethodParameters`: formal parameter names (`None` - unnamed).
    MethodParameters(Vec<MethodParameter>),
    /// `Record`: the record class's components.
    Record(Vec<RecordComponent>),
    /// `LocalVariableTypeTable`: generic local signatures (like LVT).
    LocalVariableTypeTable(Vec<LocalVariableType>),
    /// `RuntimeVisibleAnnotations` (JVMS §4.7.16): parsed, not raw bytes -
    /// reflection reads them at runtime and raw indices go stale on write.
    RuntimeVisibleAnnotations(Vec<Annotation>),
    /// `RuntimeInvisibleAnnotations` (JVMS §4.7.17): CLASS-retention ones.
    RuntimeInvisibleAnnotations(Vec<Annotation>),
    /// `RuntimeVisibleParameterAnnotations` (JVMS §4.7.18): per-parameter
    /// annotations. Parsed like the class-level ones - raw pool indices go
    /// stale when the pool is rebuilt.
    RuntimeVisibleParameterAnnotations(Vec<Vec<Annotation>>),
    /// `RuntimeInvisibleParameterAnnotations` (JVMS §4.7.19).
    RuntimeInvisibleParameterAnnotations(Vec<Vec<Annotation>>),
    /// `RuntimeVisibleTypeAnnotations` (JVMS §4.7.20): type-use
    /// annotations. Parsed - raw pool indices go stale on write and ASM
    /// dies with `ArrayIndexOutOfBoundsException` reading them back.
    RuntimeVisibleTypeAnnotations(Vec<TypeAnnotation>),
    /// `RuntimeInvisibleTypeAnnotations` (JVMS §4.7.21).
    RuntimeInvisibleTypeAnnotations(Vec<TypeAnnotation>),
    /// `AnnotationDefault` (JVMS §4.7.22): the default value of one
    /// annotation member - a single `element_value` with pool references.
    AnnotationDefault(AnnotationValue),
    /// Everything else: name + raw `info` bytes (careful with pool indices).
    Raw {
        name: String,
        data: Vec<u8>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineNumber {
    /// Start label (the writer computes bytes - survives pass insertions).
    pub start: Label,
    pub line: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LocalVariable {
    /// Visibility-range labels (the writer recomputes them into bytes).
    pub start: Label,
    pub end: Label,
    pub name: String,
    pub descriptor: String,
    pub index: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InnerClass {
    pub inner: String,
    pub outer: Option<String>,
    pub inner_name: Option<String>,
    pub access_flags: u16,
}

/// One `bootstrap` method: a `MethodHandle`-style reference + const arguments.
#[derive(Debug, Clone, PartialEq)]
pub struct BootstrapMethod {
    /// The `reference_kind` number (JVMS §5.4.3.5).
    pub kind: u8,
    pub owner: String,
    pub name: String,
    pub descriptor: String,
    pub itf: bool,
    pub args: Vec<Const>,
}

/// A method's formal parameter (`MethodParameters`).
#[derive(Debug, Clone, PartialEq)]
pub struct MethodParameter {
    pub name: Option<String>,
    pub access_flags: u16,
}

/// A `Record` component: a record class field with attributes.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordComponent {
    pub name: String,
    pub descriptor: String,
    pub attributes: Vec<Attribute>,
}

/// A `LocalVariableTypeTable` entry (like `LocalVariable`, but with a signature).
#[derive(Debug, Clone, PartialEq)]
pub struct LocalVariableType {
    /// Visibility-range labels (the writer recomputes them into bytes).
    pub start: Label,
    pub end: Label,
    pub name: String,
    pub signature: String,
    pub index: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub access_flags: u16,
    pub name: String,
    pub descriptor: String,
    pub attributes: Vec<Attribute>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Method {
    pub access_flags: u16,
    pub name: String,
    pub descriptor: String,
    pub code: Option<Code>,
    pub attributes: Vec<Attribute>,
}

impl Method {
    /// `native`/`abstract` methods have no body.
    pub fn has_code(&self) -> bool {
        self.code.is_some()
    }
}

/// A method body.
#[derive(Debug, Clone, PartialEq)]
pub struct Code {
    pub max_stack: u16,
    pub max_locals: u16,
    /// Label markers + instructions; branches point at a [`Label`].
    pub insns: Vec<Insn>,
    pub exception_table: Vec<ExceptionHandler>,
    pub frames: Vec<StackMapFrame>,
    pub attributes: Vec<Attribute>,
}

impl Code {
    /// All labels declared in the method body.
    pub fn labels(&self) -> Vec<Label> {
        self.insns
            .iter()
            .filter_map(|i| match i {
                Insn::Mark(l) => Some(*l),
                _ => None,
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExceptionHandler {
    /// `None` - catch-all (`finally`).
    pub start: Label,
    pub end: Label,
    pub handler: Label,
    /// The class's internal name, or `None` for `any`.
    pub catch_type: Option<String>,
}

/// One `.class` in normalized form.
#[derive(Debug, Clone, PartialEq)]
pub struct ClassFile {
    pub version: Version,
    pub access_flags: u16,
    pub this_class: String,
    pub super_class: Option<String>,
    pub interfaces: Vec<String>,
    pub fields: Vec<Field>,
    pub methods: Vec<Method>,
    pub attributes: Vec<Attribute>,
}

impl ClassFile {
    /// Finds a method by name and descriptor.
    pub fn method(&self, name: &str, descriptor: &str) -> Option<&Method> {
        self.methods
            .iter()
            .find(|m| m.name == name && m.descriptor == descriptor)
    }

    /// Finds a method by name and descriptor (mutating access).
    pub fn method_mut(&mut self, name: &str, descriptor: &str) -> Option<&mut Method> {
        self.methods
            .iter_mut()
            .find(|m| m.name == name && m.descriptor == descriptor)
    }

    /// The `ACC_INTERFACE` flag.
    pub fn is_interface(&self) -> bool {
        self.access_flags & 0x0200 != 0
    }
}

/// A resource inside the archive (JSON, properties, images, ...).
#[derive(Debug, Clone, PartialEq)]
pub struct Resource {
    pub path: String,
    pub data: Vec<u8>,
}

/// The whole input program: classes + resources.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Program {
    pub classes: Vec<ClassFile>,
    pub resources: Vec<Resource>,
}

impl Program {
    /// A class by internal name (`pkg/Class`).
    pub fn class(&self, internal_name: &str) -> Option<&ClassFile> {
        self.classes.iter().find(|c| c.this_class == internal_name)
    }

    /// A class by internal name (mutating access).
    pub fn class_mut(&mut self, internal_name: &str) -> Option<&mut ClassFile> {
        self.classes
            .iter_mut()
            .find(|c| c.this_class == internal_name)
    }

    /// A resource by its path inside the archive.
    pub fn resource(&self, path: &str) -> Option<&Resource> {
        self.resources.iter().find(|r| r.path == path)
    }

    /// Whether the program is empty.
    pub fn is_empty(&self) -> bool {
        self.classes.is_empty() && self.resources.is_empty()
    }
}
