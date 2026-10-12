//! Fabric mixins.json and refmap: rewrite package + all references.
//!
//! Mixins reference classes by short names relative to `package`.
//! Plain full-name text replacement does not work. Algorithm:
//! 1. Collect full paths of all mixin entries (package + short name).
//! 2. Find common prefix of mapped names.
//! 3. Rewrite `package` to that prefix, entries relative to it.

use std::collections::BTreeMap;

/// Patch mixins.json: package, entries in mixins/client/server, plugin.
pub fn patch_mixins_json(text: &str, map: &BTreeMap<String, String>) -> Option<String> {
    let mut root: serde_json::Value = serde_json::from_str(text).ok()?;

    let pkg_dot = root.get("package").and_then(|v| v.as_str()).unwrap_or("");
    let pkg_path = pkg_dot.replace('.', "/");

    let sections = ["mixins", "client", "server"];

    // 1. Collect full paths of all entries.
    let mut full_names: Vec<String> = Vec::new();
    for section in sections {
        let Some(arr) = root.get(section).and_then(|v| v.as_array()) else {
            continue;
        };
        for el in arr {
            let Some(short) = el.as_str() else { continue };
            let full = if pkg_path.is_empty() {
                short.replace('.', "/")
            } else {
                format!("{}/{}", pkg_path, short.replace('.', "/"))
            };
            let mapped = map.get(&full).cloned().unwrap_or(full);
            full_names.push(mapped);
        }
    }

    // 2. Common prefix of mapped names.
    let common = common_prefix(&full_names);
    let common_dot = common.replace('/', ".");

    // 3. Rewrite package and entries.
    for section in sections {
        let Some(arr) = root.get(section).and_then(|v| v.as_array()) else {
            continue;
        };
        let entries: Vec<String> = arr
            .iter()
            .filter_map(|el| el.as_str())
            .map(|short| {
                let full = if pkg_path.is_empty() {
                    short.replace('.', "/")
                } else {
                    format!("{}/{}", pkg_path, short.replace('.', "/"))
                };
                let mapped = map.get(&full).cloned().unwrap_or(full);
                let tail = if !common.is_empty() && mapped.starts_with(&format!("{common}/")) {
                    &mapped[common.len() + 1..]
                } else {
                    &mapped
                };
                tail.replace('/', ".")
            })
            .collect();
        root[section] =
            serde_json::Value::Array(entries.into_iter().map(serde_json::Value::String).collect());
    }
    root["package"] = serde_json::Value::String(common_dot);

    // 4. Plugin (full class name).
    if let Some(plugin) = root.get("plugin").and_then(|v| v.as_str()) {
        let cls = plugin.replace('.', "/");
        if let Some(mapped) = map.get(&cls) {
            root["plugin"] = serde_json::Value::String(mapped.replace('/', "."));
        }
    }

    Some(serde_json::to_string_pretty(&root).unwrap_or_else(|_| text.to_string()))
}

/// Patch refmap.json: recursively replace all string values.
pub fn patch_refmap_json(text: &str, map: &BTreeMap<String, String>) -> Option<String> {
    let mut root: serde_json::Value = serde_json::from_str(text).ok()?;
    remap_json_value(&mut root, map);
    Some(serde_json::to_string_pretty(&root).unwrap_or_else(|_| text.to_string()))
}

/// Recursively replace strings in JSON via mapping.
fn remap_json_value(value: &mut serde_json::Value, map: &BTreeMap<String, String>) {
    match value {
        serde_json::Value::Object(obj) => {
            let remapped: serde_json::Map<String, serde_json::Value> = obj
                .iter()
                .map(|(key, val)| {
                    let new_key = remap_reference(key, map);
                    let mut new_val = val.clone();
                    remap_json_value(&mut new_val, map);
                    (new_key, new_val)
                })
                .collect();
            *obj = remapped;
        }
        serde_json::Value::Array(arr) => {
            for el in arr.iter_mut() {
                remap_json_value(el, map);
            }
        }
        serde_json::Value::String(s) => {
            *s = remap_reference(s, map);
        }
        _ => {}
    }
}

/// Replace class/method/field reference in a string.
fn remap_reference(s: &str, map: &BTreeMap<String, String>) -> String {
    // Internal name (pkg/Class) - return internal form.
    if let Some(mapped) = map.get(s) {
        return mapped.clone();
    }
    // Dotted name (pkg.Class) - return dotted form.
    let internal = s.replace('.', "/");
    if internal != s
        && let Some(mapped) = map.get(&internal)
    {
        return mapped.replace('/', ".");
    }
    s.to_string()
}

/// Find common path prefix of names (separator `/`).
fn common_prefix(names: &[String]) -> String {
    if names.is_empty() {
        return String::new();
    }
    let parts: Vec<Vec<&str>> = names.iter().map(|s| s.split('/').collect()).collect();
    let first = &parts[0];
    if first.len() <= 1 {
        return String::new();
    }
    let mut common = first.len() - 1;
    for cur in parts.iter().skip(1) {
        let mut k = 0;
        while k < common && k < cur.len() && first[k] == cur[k] {
            k += 1;
        }
        common = k;
    }
    if common == 0 {
        return String::new();
    }
    first[..common].join("/")
}
