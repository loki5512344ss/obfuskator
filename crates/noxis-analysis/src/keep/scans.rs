//! Program scans feeding keep rules: bootstrap methods, databind literals, mixins.

use noxis_classfile::{Attribute, Const, Insn, Program};

use super::KeepRules;

impl KeepRules {
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

    /// Lock every Mixin class (and the plugin) listed in the configs.
    ///
    /// The Mixin framework resolves mixin classes and their members by name
    /// at runtime, so renaming them breaks the mod at startup. Entries use
    /// simple names relative to the `package` field, which generic text
    /// patching can never see - locking is the only sound answer. Locking
    /// the class also freezes its members (locked classes skip member
    /// renaming entirely), which the `@Accessor`/`@Invoker`/`@Overwrite`
    /// lookups need. Mixins configs are found two ways: any resource ending
    /// in `mixins.json`, plus whatever `fabric.mod.json` lists.
    ///
    /// DISABLED: the resources pass now patches mixins.json properly
    /// (rewrites package + entries via common prefix), so mixin classes
    /// can be renamed like any other class. Reference: OBF fabric-obfuscator.
    #[allow(dead_code)]
    pub fn absorb_mixins(&mut self, program: &Program) {
        let mut listed: Vec<String> = Vec::new();
        for resource in &program.resources {
            if !resource.path.ends_with("fabric.mod.json") {
                continue;
            }
            if let Ok(text) = std::str::from_utf8(&resource.data) {
                listed.extend(mixin_config_names(text));
            }
        }
        for resource in &program.resources {
            let is_mixins = resource.path.ends_with("mixins.json");
            let is_listed = listed.iter().any(|name| resource.path.ends_with(name));
            if !is_mixins && !is_listed {
                continue;
            }
            if let Ok(text) = std::str::from_utf8(&resource.data) {
                for class in mixin_classes(text) {
                    if program.class(&class).is_some() {
                        self.keep_exact(class);
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

/// Filenames from a `fabric.mod.json` "mixins" array.
fn mixin_config_names(text: &str) -> Vec<String> {
    let Ok(json) = serde_json::from_str::<serde_json::Value>(text) else {
        return Vec::new();
    };
    json.get("mixins")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Mixin classes (internal names) from one mixins config.
///
/// Entries may be simple (relative to `package`) or already qualified;
/// both spellings are returned and the caller keeps whichever exists.
/// The `plugin` class is included the same way.
fn mixin_classes(text: &str) -> Vec<String> {
    let Ok(json) = serde_json::from_str::<serde_json::Value>(text) else {
        return Vec::new();
    };
    let package = json
        .get("package")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .replace('.', "/");
    let mut out = Vec::new();
    let push = |entry: &str, out: &mut Vec<String>| {
        out.push(entry.replace('.', "/"));
        if !package.is_empty() {
            out.push(format!("{package}/{}", entry.replace('.', "/")));
        }
    };
    for key in ["mixins", "client", "server"] {
        if let Some(items) = json.get(key).and_then(|value| value.as_array()) {
            for item in items {
                if let Some(entry) = item.as_str() {
                    push(entry, &mut out);
                }
            }
        }
    }
    if let Some(plugin) = json.get("plugin").and_then(|value| value.as_str()) {
        push(plugin, &mut out);
    }
    out
}
