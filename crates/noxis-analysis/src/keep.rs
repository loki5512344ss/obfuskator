//! Keep-правила: что нельзя переименовывать (glob-паттерны + entrypoint'ы).

use regex::Regex;
use noxis_classfile::{Attribute, Const, Program};

use crate::entrypoints::Entrypoints;
use noxis_core::Config;

/// Скомпилированный glob: `*` — любые символы кроме `/`, `**` — всё, `?` — один.
fn compile(pattern: &str) -> Result<Regex, regex::Error> {
    let mut out = String::from("^");
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '*' => {
                if i + 1 < chars.len() && chars[i + 1] == '*' {
                    out.push_str(".*");
                    i += 2;
                } else {
                    out.push_str("[^/]*");
                    i += 1;
                }
            }
            '?' => {
                out.push_str("[^/]");
                i += 1;
            }
            c => {
                out.push_str(&regex::escape(&c.to_string()));
                i += 1;
            }
        }
    }
    out.push('$');
    Regex::new(&out)
}

/// Правила «не трогать»: явные паттерны конфига + entrypoint'ы.
#[derive(Debug, Default, Clone)]
pub struct KeepRules {
    class_patterns: Vec<Regex>,
    resource_patterns: Vec<Regex>,
    /// Точные имена, которые нельзя трогать (entrypoint'ы, рефлексия, ...).
    fixed: std::collections::BTreeSet<String>,
    /// Имена членов, защищённые рефлексией.
    fixed_members: std::collections::BTreeSet<String>,
}

impl KeepRules {
    /// Собрать правила из конфига.
    pub fn from_config(config: &Config) -> Result<Self, RuleError> {
        let mut class_patterns = Vec::with_capacity(config.keep.len());
        for pattern in &config.keep {
            class_patterns.push(compile(pattern).map_err(|e| RuleError::Pattern {
                pattern: pattern.clone(),
                msg: e.to_string(),
            })?);
        }
        let mut resource_patterns = Vec::with_capacity(config.keep_resources.len());
        for pattern in &config.keep_resources {
            resource_patterns.push(compile(pattern).map_err(|e| RuleError::Pattern {
                pattern: pattern.clone(),
                msg: e.to_string(),
            })?);
        }
        Ok(Self {
            class_patterns,
            resource_patterns,
            fixed: std::collections::BTreeSet::new(),
            fixed_members: std::collections::BTreeSet::new(),
        })
    }

    /// Запретить переименование члена с таким именем (в любом классе).
    pub fn keep_member(&mut self, name: impl Into<String>) {
        self.fixed_members.insert(name.into());
    }

    /// Добавить точное имя класса (из анализа программы).
    pub fn keep_exact(&mut self, internal_name: impl Into<String>) {
        self.fixed.insert(internal_name.into());
    }

    /// Объединить с найденными entrypoint'ами.
    pub fn absorb(&mut self, entrypoints: &Entrypoints) {
        for name in &entrypoints.main_classes {
            self.keep_exact(name.clone());
        }
        for name in &entrypoints.reflective_names {
            self.keep_exact(name.clone());
        }
        for name in &entrypoints.serializable_classes {
            self.keep_exact(name.clone());
        }
        for (owner, _, _) in &entrypoints.native_methods {
            self.keep_exact(owner.clone());
        }
        for name in &entrypoints.service_interfaces {
            self.keep_exact(name.clone());
        }
        for name in &entrypoints.reflective_member_names {
            self.keep_member(name.clone());
        }
    }

    /// Методы, на которые ссылаются `BootstrapMethods` (`invokedynamic`).
    ///
    /// Переименование такого метода без правки bootstrap-записи ломает класс,
    /// а правка — задача P2 (см. `todo.md`): пока просто не трогаем имена.
    pub fn absorb_bootstrap(&mut self, program: &Program) {
        for class in &program.classes {
            for method in &class.methods {
                for attr in &method.attributes {
                    if let Attribute::BootstrapMethods(methods) = attr {
                        for bsm in methods {
                            self.keep_member(bsm.name.clone());
                            for arg in &bsm.args {
                                if let Const::MethodHandle { name, .. } = arg {
                                    self.keep_member(name.clone());
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Нужно ли оставить класс с данным внутренним именем.
    pub fn keeps_class(&self, internal_name: &str) -> bool {
        self.fixed.contains(internal_name)
            || self
                .class_patterns
                .iter()
                .any(|re| re.is_match(internal_name))
    }

    /// Нужно ли оставить член с таким именем (не переименовывать).
    pub fn keeps_member(&self, name: &str) -> bool {
        self.fixed_members.contains(name)
    }

    /// Нужно ли оставить ресурс с данным путём.
    pub fn keeps_resource(&self, path: &str) -> bool {
        self.resource_patterns.iter().any(|re| re.is_match(path))
    }

    /// Число правил (для отчёта).
    pub fn len(&self) -> usize {
        self.class_patterns.len()
            + self.resource_patterns.len()
            + self.fixed.len()
            + self.fixed_members.len()
    }

    /// Есть ли вообще правила.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Ошибка компиляции паттерна.
#[derive(Debug, thiserror::Error)]
pub enum RuleError {
    #[error("некорректный паттерн `{pattern}`: {msg}")]
    Pattern { pattern: String, msg: String },
}

/// Полный анализ перед обфускацией.
#[derive(Debug, Clone)]
pub struct Analysis {
    pub entrypoints: Entrypoints,
    pub keep: KeepRules,
    /// Классы, которые не переименовываются.
    pub locked_classes: std::collections::BTreeSet<String>,
}

impl Analysis {
    /// Прогнать весь анализ по программе.
    pub fn run(program: &Program, config: &Config) -> Result<Self, RuleError> {
        let entrypoints = Entrypoints::collect(program);
        let mut keep = KeepRules::from_config(config)?;
        keep.absorb(&entrypoints);
        keep.absorb_bootstrap(program);

        let locked_classes = program
            .classes
            .iter()
            .map(|c| c.this_class.clone())
            .filter(|name| keep.keeps_class(name))
            .collect();

        Ok(Self {
            entrypoints,
            keep,
            locked_classes,
        })
    }

    /// Класс запрещено переименовывать.
    pub fn is_locked(&self, internal_name: &str) -> bool {
        self.locked_classes.contains(internal_name)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]
    use super::*;

    #[test]
    fn glob_semantics() {
        let rules = KeepRules {
            class_patterns: vec![
                compile("com/example/*").unwrap(),
                compile("**/internal/**").unwrap(),
            ],
            resource_patterns: vec![compile("META-INF/services/*").unwrap()],
            fixed: std::collections::BTreeSet::new(),
            fixed_members: std::collections::BTreeSet::new(),
        };
        assert!(rules.keeps_class("com/example/Main"));
        assert!(!rules.keeps_class("com/example/sub/Main"));
        assert!(rules.keeps_class("x/internal/y/Z"));
        assert!(rules.keeps_resource("META-INF/services/a.b.C"));
        assert!(!rules.keeps_resource("assets/mc/mod.json"));
    }

    #[test]
    fn exact_names_win() {
        let mut rules = KeepRules::default();
        rules.keep_exact("app/Main");
        assert!(rules.keeps_class("app/Main"));
        assert!(!rules.keeps_class("app/Other"));
    }
}
