//! Config sections: name styles and per-pass settings.

use serde::{Deserialize, Serialize};

/// Name generation style.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NameStyle {
    /// `a`, `b`, ... `aa` — short Latin.
    ShortLatin,
    /// `I`, `l`, `O`, `0` — visually identical.
    IlO0,
    /// Random Unicode names (surrogates are valid in modified UTF-8).
    Unicode,
    /// Medium-length mixed-case Latin.
    #[default]
    MixedLatin,
    /// Pseudo-meaningful words from another project (misdirection).
    DecoyWords,
    /// Plausible names: `TextureCache`, `resolveToken` — blends in as
    /// ordinary application code instead of obvious noise.
    Plausible,
}

/// Rename pass settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RenameConfig {
    /// Scatter packages into random paths.
    pub spread_packages: bool,
    /// Size of the random package pool.
    pub package_pool: usize,
    /// Name style for classes.
    pub class_style: NameStyle,
    /// Name style for fields and methods.
    pub member_style: NameStyle,
    /// Name style for local variables and parameters.
    pub local_style: NameStyle,
    /// Widen members to `public` (so moving to a new package doesn't break
    /// inheritance).
    pub widen_access: bool,
    /// Rename enum constants.
    pub rename_enum_constants: bool,
    /// Rename local variables and parameters.
    pub rename_locals: bool,
    /// Also rename non-private members (aggressive: safe for applications,
    /// dangerous for libraries and reflection-heavy code).
    pub rename_public_members: bool,
}

impl Default for RenameConfig {
    fn default() -> Self {
        Self {
            spread_packages: true,
            package_pool: 12,
            class_style: NameStyle::ShortLatin,
            member_style: NameStyle::MixedLatin,
            local_style: NameStyle::IlO0,
            widen_access: true,
            rename_enum_constants: true,
            rename_locals: true,
            rename_public_members: false,
        }
    }
}

/// String encryption settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StringsConfig {
    /// Cipher schemes strings are rotated across.
    pub schemes: Vec<String>,
    /// Strings shorter than this are not encrypted.
    pub min_len: usize,
    /// Split long strings into chunks (looks less like a string pool).
    pub split_above: usize,
    /// Don't encrypt strings matching these patterns.
    pub skip: Vec<String>,
    /// Derive the key from KDF(class, method, index) instead of placing it
    /// next to the ciphertext.
    pub derive_key: bool,
}

impl Default for StringsConfig {
    fn default() -> Self {
        Self {
            schemes: vec!["xor-splitmix".to_string(), "xtea-ctr".to_string()],
            min_len: 2,
            split_above: 64,
            skip: Vec::new(),
            derive_key: true,
        }
    }
}

/// Control-flow obfuscation settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FlowConfig {
    /// Schemes picked randomly per method.
    pub schemes: Vec<String>,
    /// Don't touch methods larger than this many instructions (size budget).
    pub max_insns: usize,
    /// Weave opaque predicates into block bodies.
    pub encrypt_block_bodies: bool,
}

impl Default for FlowConfig {
    fn default() -> Self {
        Self {
            schemes: vec![
                "state-spread".to_string(),
                "computed-switch".to_string(),
                "butterfly".to_string(),
                "exception-goto".to_string(),
            ],
            max_insns: 4000,
            encrypt_block_bodies: false,
        }
    }
}

/// Constant encryption settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ConstantsConfig {
    /// Splitting schemes (`xor`, `add`, `shift`), picked by RNG per constant.
    pub schemes: Vec<String>,
    /// Insert opaque predicates (at most this many per method).
    pub max_predicates: usize,
    /// Don't touch constants in these classes (internal-name prefixes).
    pub skip: Vec<String>,
}

impl Default for ConstantsConfig {
    fn default() -> Self {
        Self {
            schemes: vec!["xor".to_string(), "add".to_string(), "shift".to_string()],
            max_predicates: 1,
            skip: Vec::new(),
        }
    }
}

/// Noise pass settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NoiseConfig {
    /// How many junk classes per real one (0 = disabled).
    pub junk_ratio: usize,
    /// Number of junk annotations per class.
    pub junk_annotations: usize,
    /// Weave junk classes into the reachability graph (required for them to
    /// make sense).
    pub weave_reachable: bool,
}

impl Default for NoiseConfig {
    fn default() -> Self {
        Self {
            junk_ratio: 0,
            junk_annotations: 0,
            weave_reachable: true,
        }
    }
}

/// Debug info pass settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DebugConfig {
    /// Strip `LineNumberTable`.
    pub strip_line_numbers: bool,
    /// Strip `LocalVariableTable`.
    pub strip_local_variables: bool,
    /// Replace `SourceFile` with a fake name.
    pub spoof_source_file: bool,
    /// Strip `Signature`/`InnerClasses`/`EnclosingMethod`.
    pub strip_signatures: bool,
}

impl Default for DebugConfig {
    fn default() -> Self {
        Self {
            strip_line_numbers: true,
            strip_local_variables: true,
            spoof_source_file: true,
            strip_signatures: false,
        }
    }
}
