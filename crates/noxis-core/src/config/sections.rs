//! Секции конфигурации: стили имён и настройки отдельных пассов.

use serde::{Deserialize, Serialize};

/// Стиль генерации имён.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NameStyle {
    /// `a`, `b`, ... `aa` — короткие латинские.
    ShortLatin,
    /// `I`, `l`, `O`, `0` — визуально одинаковые.
    IlO0,
    /// Случайные unicode-имена (суррогаты допустимы в modified UTF-8).
    Unicode,
    /// Смешанная латиница средней длины.
    #[default]
    MixedLatin,
    /// Псевдосмысленные слова из другого проекта (дезориентация).
    DecoyWords,
    /// Правдоподобные имена: `TextureCache`, `resolveToken` — вместо
    /// явного мусора маскируется под обычный прикладной код.
    Plausible,
}

/// Настройки переименования.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RenameConfig {
    /// Переименовывать пакеты в случайные пути.
    pub spread_packages: bool,
    /// Размер пула случайных пакетов.
    pub package_pool: usize,
    /// Стиль имён классов.
    pub class_style: NameStyle,
    /// Стиль имён полей и методов.
    pub member_style: NameStyle,
    /// Стиль имён локальных переменных и параметров.
    pub local_style: NameStyle,
    /// Делать члены `public` (чтобы не ломать inheritance между пакетами).
    pub widen_access: bool,
    /// Переименовывать enum-константы.
    pub rename_enum_constants: bool,
    /// Переименовывать локальные переменные и параметры.
    pub rename_locals: bool,
    /// Переименовывать и не-private члены (агрессивно: безопасно для приложений,
    /// опасно для библиотек и кода с рефлексией).
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

/// Настройки шифрования строк.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StringsConfig {
    /// Схемы шифра, между которыми перемешиваются строки.
    pub schemes: Vec<String>,
    /// Строки короче этого не шифруются.
    pub min_len: usize,
    /// Дробить длинные строки на чанки (меньше похоже на пул).
    pub split_above: usize,
    /// Не шифровать строки, совпадающие с этими паттернами.
    pub skip: Vec<String>,
    /// Вычислять ключ из KDF(class, method, index), а не класть рядом с текстом.
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

/// Настройки обфускации контрольного потока.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FlowConfig {
    /// Схемы, между которыми выбирается случайно по методу.
    pub schemes: Vec<String>,
    /// Не трогать методы больше этого числа инструкций (бюджет размера).
    pub max_insns: usize,
    /// Вплетать opaque-предикаты в тела блоков.
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

/// Настройки шифрования констант.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ConstantsConfig {
    /// Схемы разбиения (`xor`, `add`, `shift`), выбор по RNG на константу.
    pub schemes: Vec<String>,
    /// Вставлять opaque-предикаты (не больше этого на метод).
    pub max_predicates: usize,
    /// Не трогать константы в этих классах (префиксы внутренних имён).
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

/// Настройки шума.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NoiseConfig {
    /// Сколько junk-классов на каждый настоящий (0 = выключено).
    pub junk_ratio: usize,
    /// Число junk-аннотаций на класс.
    pub junk_annotations: usize,
    /// Вплетать junk-классы в граф достижимости (обязательно для осмысленности).
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

/// Настройки отладочной информации.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DebugConfig {
    /// Удалять `LineNumberTable`.
    pub strip_line_numbers: bool,
    /// Удалять `LocalVariableTable`.
    pub strip_local_variables: bool,
    /// Подменять `SourceFile` на ложное имя.
    pub spoof_source_file: bool,
    /// Удалять `Signature`/`InnerClasses`/`EnclosingMethod`.
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
