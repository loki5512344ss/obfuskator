//! Keep rules: what must not be renamed (glob patterns + entrypoints).

use std::collections::BTreeSet;

use noxis_classfile::{Attribute, Const, Insn, Program};
use regex::Regex;

use crate::entrypoints::Entrypoints;
use noxis_core::Config;

/// Compiled glob: `*` - anything but `/`, `**` - anything, `?` - one char.
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

/// Databinding entry points: `(owner, method names)` taking a `Class` literal.
/// Gson, Jackson, fastjson2 and Moshi bind DTO fields by reflection from
/// `X.class`, with no field-name string for the string-based detection.
const DATABIND_CALLS: [(&str, &[&str]); 5] = [
    ("com/google/gson/Gson", &["fromJson"]),
    (
        "com/fasterxml/jackson/databind/ObjectMapper",
        &["readValue", "readValues", "convertValue"],
    ),
    (
        "com/fasterxml/jackson/databind/ObjectReader",
        &["readValue", "readValues"],
    ),
    ("com/alibaba/fastjson2/JSON", &["parseObject"]),
    ("com/squareup/moshi/Moshi", &["adapter"]),
];

/// How far back an `X.class` literal may sit before the databind call.
/// Argument setup is straight-line: a few loads between the literal and the
/// call at most.
const CLASS_LITERAL_WINDOW: usize = 10;

/// "Do not touch" rules: explicit config patterns + entrypoints.
#[derive(Debug, Default, Clone)]
pub struct KeepRules {
    class_patterns: Vec<Regex>,
    resource_patterns: Vec<Regex>,
    /// Exact names to keep (entrypoints, reflection, ...).
    fixed: BTreeSet<String>,
    /// Member names protected by reflection.
    fixed_members: BTreeSet<String>,
}

impl KeepRules {
    /// Build the rules from the config.
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
            fixed: BTreeSet::new(),
            fixed_members: BTreeSet::new(),
        })
    }

    /// Forbid renaming a member with this name (in any class).
    pub fn keep_member(&mut self, name: impl Into<String>) {
        self.fixed_members.insert(name.into());
    }

    /// Add an exact class name (from program analysis).
    pub fn keep_exact(&mut self, internal_name: impl Into<String>) {
        self.fixed.insert(internal_name.into());
    }

    /// Absorb found entrypoints.
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

    /// Methods referenced by `BootstrapMethods` (`invokedynamic`).
    ///
    /// Renaming such a method without patching the bootstrap record breaks
    /// the class, and patching is a P2 task (see `todo.md`): keep the names.
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

    /// Lock every field of every class passed as an `X.class` literal to a
    /// databinding call. Gson/Jackson bind fields by reflection from the
    /// literal; renaming them silently breaks deserialization, while the
    /// class itself (never named in JSON) still renames freely.
    pub fn absorb_databind(&mut self, program: &Program) {
        for class in &program.classes {
            for method in &class.methods {
                let Some(code) = method.code.as_ref() else {
                    continue;
                };
                for (index, insn) in code.insns.iter().enumerate() {
                    let Insn::Invoke { owner, name, .. } = insn else {
                        continue;
                    };
                    if !is_databind_call(owner, name) {
                        continue;
                    }
                    if let Some(dto) = nearest_class_literal(&code.insns[..index])
                        && let Some(dto_class) = program.class(&dto)
                    {
                        for field in &dto_class.fields {
                            self.keep_member(field.name.clone());
                        }
                    }
                }
            }
        }
    }

    /// Whether the class with this internal name stays.
    pub fn keeps_class(&self, internal_name: &str) -> bool {
        self.fixed.contains(internal_name)
            || self
                .class_patterns
                .iter()
                .any(|re| re.is_match(internal_name))
    }

    /// Whether a member with this name stays (is not renamed).
    pub fn keeps_member(&self, name: &str) -> bool {
        self.fixed_members.contains(name)
    }

    /// Whether the resource at this path stays unrewritten.
    pub fn keeps_resource(&self, path: &str) -> bool {
        self.resource_patterns.iter().any(|re| re.is_match(path))
    }

    /// Rule count (for the report).
    pub fn len(&self) -> usize {
        self.class_patterns.len()
            + self.resource_patterns.len()
            + self.fixed.len()
            + self.fixed_members.len()
    }

    /// Whether there are any rules at all.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Whether an invoke is a databinding call taking a `Class` literal.
fn is_databind_call(owner: &str, name: &str) -> bool {
    DATABIND_CALLS
        .iter()
        .any(|(candidate, names)| *candidate == owner && names.contains(&name))
}

/// The nearest `X.class` literal before a call site (control-flow
/// boundaries stop the search: argument setup is straight-line).
fn nearest_class_literal(before: &[Insn]) -> Option<String> {
    let start = before.len().saturating_sub(CLASS_LITERAL_WINDOW);
    for insn in before[start..].iter().rev() {
        match insn {
            Insn::Const(Const::Class(name)) => return Some(name.clone()),
            Insn::Branch(..)
            | Insn::Goto(_)
            | Insn::TableSwitch { .. }
            | Insn::LookupSwitch { .. }
            | Insn::Jsr(_)
            | Insn::Ret(_)
            | Insn::Return(_)
            | Insn::AReturn
            | Insn::AThrow => return None,
            _ => {}
        }
    }
    None
}

/// A pattern compilation error.
#[derive(Debug, thiserror::Error)]
pub enum RuleError {
    #[error("malformed pattern `{pattern}`: {msg}")]
    Pattern { pattern: String, msg: String },
}

/// The full pre-obfuscation analysis.
#[derive(Debug, Clone)]
pub struct Analysis {
    pub entrypoints: Entrypoints,
    pub keep: KeepRules,
    /// Classes that are not renamed.
    pub locked_classes: BTreeSet<String>,
}

impl Analysis {
    /// Run the whole analysis over the program.
    pub fn run(program: &Program, config: &Config) -> Result<Self, RuleError> {
        let entrypoints = Entrypoints::collect(program);
        let mut keep = KeepRules::from_config(config)?;
        keep.absorb(&entrypoints);
        keep.absorb_bootstrap(program);
        keep.absorb_databind(program);

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

    /// Renaming this class is forbidden.
    pub fn is_locked(&self, internal_name: &str) -> bool {
        self.locked_classes.contains(internal_name)
    }
}
