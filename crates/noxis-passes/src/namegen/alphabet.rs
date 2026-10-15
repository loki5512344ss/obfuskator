//! Alphabets, keywords and forbidden names.
//!
//! Kept in their own file because both the generator and the rename pass
//! reach into them (reserving keep-listed names before generation starts).

/// Hard Java keywords: a class/method name can never collide with these.
pub(super) const KEYWORDS: &[&str] = &[
    "abstract",
    "assert",
    "boolean",
    "break",
    "byte",
    "case",
    "catch",
    "char",
    "class",
    "const",
    "continue",
    "default",
    "do",
    "double",
    "else",
    "enum",
    "extends",
    "final",
    "finally",
    "float",
    "for",
    "goto",
    "if",
    "implements",
    "import",
    "instanceof",
    "int",
    "interface",
    "long",
    "native",
    "new",
    "package",
    "private",
    "protected",
    "public",
    "return",
    "short",
    "static",
    "strictfp",
    "super",
    "switch",
    "synchronized",
    "this",
    "throw",
    "throws",
    "transient",
    "try",
    "void",
    "volatile",
    "while",
    "true",
    "false",
    "null",
    "_",
    "var",
    "record",
    "sealed",
    "permits",
    "yield",
];

/// Names not worth overwriting: they'd hurt readability/logging and get
/// flagged by tooling before any deobfuscation even starts.
pub(super) const AVOID_CLASS: &[&str] = &[
    "Object",
    "String",
    "Class",
    "Record",
    "Enum",
    "Cloneable",
    "Iterable",
    "Comparable",
    "Thread",
    "Runnable",
    "Override",
    "Deprecated",
    "SuppressWarnings",
];

pub(super) const AVOID_MEMBER: &[&str] = &[
    "toString",
    "equals",
    "hashCode",
    "clone",
    "wait",
    "notify",
    "notifyAll",
    "finalize",
    "main",
    "run",
    "call",
    "apply",
];

pub(super) const LATIN_LOWER: &[u8] = b"abcdefghijklmnopqrstuvwxyz";
pub(super) const LATIN_UPPER: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
/// Visually indistinguishable characters: the core of the `IlO0` style.
pub(super) const CONFUSABLE: &[u8] = b"IlO01";
/// Unicode letters valid in a Java identifier (Cyrillic/Greek/CJK).
pub(super) const UNICODE_LETTERS: &[char] = &[
    'а', 'б', 'в', 'г', 'д', 'е', 'ж', 'з', 'и', 'к', 'л', 'м', 'н', 'п', 'р', 'с', 'т', 'у', 'ф',
    'х', 'ц', 'ч', 'ш', 'щ', 'э', 'ю', 'я', 'α', 'β', 'γ', 'δ', 'ε', 'ζ', 'η', 'θ', 'λ', 'μ', 'π',
    'ρ', 'σ', 'τ', 'φ', 'ω', '日', '本', '語', '中', '文', '国',
];
pub(super) const DECOY_LEFT: &[&str] = &[
    "broken", "silent", "quiet", "amber", "cobalt", "frozen", "hollow", "vivid", "mellow",
    "scarlet",
];
pub(super) const DECOY_RIGHT: &[&str] = &[
    "mirror", "lantern", "harbor", "ember", "shadow", "quarry", "meadow", "cipher", "anchor",
    "ripple",
];

/// `Plausible` style word bank. Mixes domain-neutral software nouns with
/// terms that are boring, structural scaffolding in the client/mod
/// ecosystem noxis is actually used on (surveyed against decompiled
/// reference client/mod sources, not shipped in this repo: `Module`,
/// `Setting`, `Event`, `Mixin`, `Manager`, `Component`, `Accessor`, `Hud`,
/// `Overlay`, `Theme`, `Timer`, `Animation`, `Rotation` all show up dozens
/// of times across real client codebases). Deliberately excludes anything
/// that
/// itself reads as a "feature flag" worth an analyst's attention (no
/// `Esp`/`Aimbot`/`KillAura`/`Chams`): a decoy name that announces
/// "interesting code lives here" defeats the point of blending in.
pub(super) const PLAUSIBLE_NOUN: &[&str] = &[
    "Texture",
    "Event",
    "State",
    "Render",
    "Asset",
    "Config",
    "Session",
    "Cache",
    "Buffer",
    "Socket",
    "Stream",
    "Widget",
    "Layout",
    "Theme",
    "Profile",
    "Metric",
    "Token",
    "Handle",
    "Context",
    "Registry",
    "Request",
    "Response",
    "Payload",
    "Resource",
    "Snapshot",
    "Pipeline",
    "Queue",
    "Slot",
    "Frame",
    "Entry",
    "Module",
    "Setting",
    "Overlay",
    "Hud",
    "Component",
    "Accessor",
    "Timer",
    "Animation",
    "Rotation",
];
/// Role suffixes for classes (`NounSuffix`, e.g. `TextureCache`).
pub(super) const PLAUSIBLE_SUFFIX: &[&str] = &[
    "Manager",
    "Handler",
    "Service",
    "Provider",
    "Factory",
    "Builder",
    "Cache",
    "Store",
    "Loader",
    "Parser",
    "Worker",
    "Controller",
    "Adapter",
    "Wrapper",
    "Registry",
    "Resolver",
    "Validator",
    "Processor",
    "Context",
    "Listener",
    "Mixin",
    "Component",
];
/// Verbs for methods/fields (`verbNoun`, e.g. `resolveToken`).
pub(super) const PLAUSIBLE_VERB: &[&str] = &[
    "get", "set", "update", "process", "handle", "compute", "resolve", "load", "build", "parse",
    "fetch", "apply", "validate", "clear", "reset", "sync", "flush", "bind", "attach", "detach",
];
