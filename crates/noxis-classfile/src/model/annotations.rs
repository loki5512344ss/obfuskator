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
