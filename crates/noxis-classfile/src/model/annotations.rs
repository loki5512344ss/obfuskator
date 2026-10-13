//! Runtime-visible/invisible annotations: names instead of pool indices.
//!
//! Raw `info` blobs go stale the moment the pool is rebuilt on write, and
//! the JVM's reflection parser then dies with `AnnotationFormatError` or
//! reads neighboring attribute bytes as signatures.

/// One `annotation` structure: the type descriptor + element-value pairs.
#[derive(Debug, Clone, PartialEq)]
pub struct Annotation {
    /// The annotation type as a field descriptor (`Lpkg/Ann;`).
    pub descriptor: String,
    /// `(element name, value)` pairs in class-file order.
    pub elements: Vec<(String, AnnotationValue)>,
}

/// One `type_annotation` (JVMS §4.7.20): a type-use annotation on a
/// method return type, a cast, a local variable, and so on.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeAnnotation {
    /// `target_info` + `type_path` bytes (§4.7.20.1-2): no pool references
    /// there at all, kept opaque so write copies them verbatim.
    pub target: Vec<u8>,
    /// The annotated type as a field descriptor (`type_index`).
    pub descriptor: String,
    /// `(element name, value)` pairs - same shape as [`Annotation`]'s.
    pub elements: Vec<(String, AnnotationValue)>,
}

/// A single `element_value`: one variant per tag, so `B`/`C`/`S`/`Z`
/// keep their identity (the pool stores them all as `CONSTANT_Integer`,
/// but HotSpot checks the tag against the annotation member's type).
#[derive(Debug, Clone, PartialEq)]
pub enum AnnotationValue {
    Byte(i32),
    Char(i32),
    Double(f64),
    Float(f32),
    Int(i32),
    Long(i64),
    Short(i32),
    Boolean(i32),
    /// The `s` tag: a `CONSTANT_Utf8`, not a `CONSTANT_String`.
    String(String),
    /// The `e` tag: enum type descriptor + constant name.
    Enum {
        descriptor: String,
        name: String,
    },
    /// The `c` tag: a class-info descriptor (`Ljava/lang/String;` or `I`).
    Class(String),
    /// The `@` tag: a nested annotation.
    Nested(Box<Annotation>),
    /// The `[` tag: array of values.
    Array(Vec<AnnotationValue>),
}
