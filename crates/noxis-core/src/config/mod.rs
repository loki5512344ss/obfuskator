//! Конфигурация обфускации: профили, список пассов, keep-правила.

pub mod sections;

use serde::{Deserialize, Serialize};

use self::sections::{
    ConstantsConfig, DebugConfig, FlowConfig, NoiseConfig, RenameConfig, StringsConfig,
};
use crate::error::CoreError;

/// Идентификатор пасса (он же ключ в конфиге, kebab-case).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PassId {
    /// Переименование классов, членов, локальных переменных, пакетов.
    Rename,
    /// Шифрование строк + генерация рантайм-декодера.
    Strings,
    /// Обфускация числовых констант и настоящие opaque-предикаты.
    Constants,
    /// Обфускация контрольного потока.
    Flow,
    /// Junk-классы, junk-аннотации, ложный шум.
    Noise,
    /// Отладочная информация: strip / spoof.
    DebugInfo,
    /// Ресурсы и метаданные архива (JSON, mixins, refmap, services, AW).
    Resources,
}

impl PassId {
    /// Строковое имя для отчётов и логов.
    pub const fn as_str(self) -> &'static str {
        match self {
            PassId::Rename => "rename",
            PassId::Strings => "strings",
            PassId::Constants => "constants",
            PassId::Flow => "flow",
            PassId::Noise => "noise",
            PassId::DebugInfo => "debug-info",
            PassId::Resources => "resources",
        }
    }
}

impl std::fmt::Display for PassId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Базовый набор пассов профиля.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Profile {
    /// Только переименование + отладка. Быстро, минимальный риск сломать код.
    Light,
    /// Стандарт: имена, строки, константы, ресурсы, отладка, шум.
    #[default]
    Standard,
    /// Всё включено, включая агрессивный control-flow.
    Paranoid,
    /// Ничего не включено явно — только `enable` из конфига.
    Manual,
}

impl Profile {
    /// Пассы, включённые профилем по умолчанию.
    pub const fn baseline(self) -> &'static [PassId] {
        match self {
            Profile::Light => &[PassId::Rename, PassId::DebugInfo, PassId::Resources],
            Profile::Standard => &[
                PassId::Rename,
                PassId::Strings,
                PassId::Constants,
                PassId::Noise,
                PassId::DebugInfo,
                PassId::Resources,
            ],
            Profile::Paranoid => &[
                PassId::Rename,
                PassId::Strings,
                PassId::Constants,
                PassId::Flow,
                PassId::Noise,
                PassId::DebugInfo,
                PassId::Resources,
            ],
            Profile::Manual => &[],
        }
    }
}

/// Полная конфигурация (TOML).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub profile: Profile,
    /// Мастер-сид; `None` — случайный, но обязательно попадает в отчёт.
    pub seed: Option<u64>,
    /// Принудительно включить пассы вне профиля.
    pub enable: Vec<PassId>,
    /// Выключить пассы профиля.
    pub disable: Vec<PassId>,
    /// Keep-паттерны внутренних имён (`com/example/Main`, `*Service`).
    pub keep: Vec<String>,
    /// Keep-паттерны путей ресурсов, которые не переписываются.
    pub keep_resources: Vec<String>,
    pub rename: RenameConfig,
    pub strings: StringsConfig,
    pub constants: ConstantsConfig,
    pub flow: FlowConfig,
    pub noise: NoiseConfig,
    pub debug: DebugConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            profile: Profile::Standard,
            seed: None,
            enable: Vec::new(),
            disable: Vec::new(),
            keep: Vec::new(),
            keep_resources: Vec::new(),
            rename: RenameConfig::default(),
            strings: StringsConfig::default(),
            constants: ConstantsConfig::default(),
            flow: FlowConfig::default(),
            noise: NoiseConfig::default(),
            debug: DebugConfig::default(),
        }
    }
}

impl Config {
    /// Включён ли пасс с учётом профиля и явных override'ов.
    pub fn is_enabled(&self, id: PassId) -> bool {
        if self.disable.contains(&id) {
            return false;
        }
        if self.enable.contains(&id) {
            return true;
        }
        self.profile.baseline().contains(&id)
    }

    /// Загрузить конфиг из TOML.
    pub fn from_toml(text: &str) -> Result<Self, CoreError> {
        toml::from_str(text).map_err(|e| CoreError::Config(e.to_string()))
    }

    /// Нормализовать значения, которые легко задать нулем/пустым списком.
    pub fn sanitize(&mut self) {
        if self.rename.package_pool == 0 {
            self.rename.package_pool = 1;
        }
        if self.strings.min_len == 0 {
            self.strings.min_len = 1;
        }
        self.strings.schemes.retain(|s| !s.trim().is_empty());
        self.flow.schemes.retain(|s| !s.trim().is_empty());
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]
    use super::*;

    #[test]
    fn profile_gate() {
        let mut cfg = Config::default();
        assert!(cfg.is_enabled(PassId::Rename));
        assert!(cfg.is_enabled(PassId::Strings));
        assert!(!cfg.is_enabled(PassId::Flow));

        cfg.profile = Profile::Paranoid;
        assert!(cfg.is_enabled(PassId::Flow));

        cfg.disable.push(PassId::Strings);
        assert!(!cfg.is_enabled(PassId::Strings));
        cfg.enable.push(PassId::Flow);
        cfg.profile = Profile::Light;
        assert!(cfg.is_enabled(PassId::Flow));
    }

    #[test]
    fn parses_toml() {
        let cfg = Config::from_toml(
            r#"
            profile = "paranoid"
            seed = 123
            keep = ["com/example/Main"]
            [rename]
            package_pool = 4
            "#,
        )
        .unwrap();
        assert_eq!(cfg.profile, Profile::Paranoid);
        assert_eq!(cfg.seed, Some(123));
        assert_eq!(cfg.rename.package_pool, 4);
    }

    #[test]
    fn rejects_unknown_keys() {
        assert!(Config::from_toml("totally_unknown = 1").is_err());
    }
}
