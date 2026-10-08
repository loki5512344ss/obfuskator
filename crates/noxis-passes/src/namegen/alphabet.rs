//! Алфавиты, ключевые слова и запрещённые имена.
//!
//! Списки вынесены отдельно: к ним обращаются и генератор, и пасс
//! переименования (резервирование keep-имён).

/// Жёсткие ключевые слова Java: имя класса/метода не может им совпадать.
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

/// Имена, которые не стоит затирать: мешают читать/логировать и ловятся
/// инструментами до любой деобфускации.
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
/// Визуально неразличимые символы — ядро стиля `IlO0`.
pub(super) const CONFUSABLE: &[u8] = b"IlO01";
/// Unicode-буквы, валидные как Java-идентификаторы (Cyrillic/Greek/CJK).
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

/// Стиль `Plausible`: домен-нейтральные существительные для классов/полей —
/// похожи на обычный прикладной код, не привязаны к конкретному проекту.
pub(super) const PLAUSIBLE_NOUN: &[&str] = &[
    "Texture", "Event", "State", "Render", "Asset", "Config", "Session", "Cache", "Buffer",
    "Socket", "Stream", "Widget", "Layout", "Theme", "Profile", "Metric", "Token", "Handle",
    "Context", "Registry", "Request", "Response", "Payload", "Resource", "Snapshot", "Pipeline",
    "Queue", "Slot", "Frame", "Entry",
];
/// Суффиксы ролей для классов (`NounSuffix`, напр. `TextureCache`).
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
];
/// Глаголы для методов/полей (`verbNoun`, напр. `resolveToken`).
pub(super) const PLAUSIBLE_VERB: &[&str] = &[
    "get", "set", "update", "process", "handle", "compute", "resolve", "load", "build", "parse",
    "fetch", "apply", "validate", "clear", "reset", "sync", "flush", "bind", "attach", "detach",
];
