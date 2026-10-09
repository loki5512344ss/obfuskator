//! Class-file version (major/minor).

/// The class-file version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Version {
    pub minor: u16,
    pub major: u16,
}

impl Version {
    /// Java 8 (52).
    pub const JAVA_8: Version = Version {
        minor: 0,
        major: 52,
    };
    /// Java 17 (61).
    pub const JAVA_17: Version = Version {
        minor: 0,
        major: 61,
    };
    /// Java 21 (65).
    pub const JAVA_21: Version = Version {
        minor: 0,
        major: 65,
    };
}
