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
