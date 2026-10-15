//! Builds [`Config`](super::Config) from a parsed [`toml_lite::Table`].
//!
//! Deliberately *not* going through `serde::Deserialize`: `Config` has
//! only a handful of fields and three small kebab-case enums, so a tiny
//! hand-written visitor is less machinery (and fewer edge cases to worry
//! about) than writing a generic `serde::Deserializer` over our `Value`
//! tree. This keeps the TOML replacement fully decoupled from serde -
//! the `Serialize`/`Deserialize` derives left on `Config` and friends are
//! unrelated to this path and can be dropped independently later.

mod sections;
#[cfg(test)]
mod tests;

use super::toml_lite::{Table, Value};
use super::{Config, PassId, Profile};

type Res<T> = Result<T, String>;

fn unknown_key(section: &str, key: &str) -> String {
    format!("неизвестный ключ `{key}` в секции `{section}`")
}

fn expect_str<'a>(v: &'a Value, path: &str) -> Res<&'a str> {
    match v {
        Value::String(s) => Ok(s),
        _ => Err(format!("`{path}` должен быть строкой")),
    }
}

fn expect_bool(v: &Value, path: &str) -> Res<bool> {
    match v {
        Value::Bool(b) => Ok(*b),
        _ => Err(format!("`{path}` должен быть bool")),
    }
}

fn expect_usize(v: &Value, path: &str) -> Res<usize> {
    match v {
        Value::Integer(i) if *i >= 0 => Ok(*i as usize),
        _ => Err(format!("`{path}` должен быть неотрицательным целым")),
    }
}

fn expect_u64(v: &Value, path: &str) -> Res<u64> {
    match v {
        Value::Integer(i) if *i >= 0 => Ok(*i as u64),
        _ => Err(format!("`{path}` должен быть неотрицательным целым")),
    }
}

fn expect_string_vec(v: &Value, path: &str) -> Res<Vec<String>> {
    match v {
        Value::Array(items) => items
            .iter()
            .map(|item| expect_str(item, path).map(str::to_string))
            .collect(),
        _ => Err(format!("`{path}` должен быть массивом строк")),
    }
}

fn parse_profile(s: &str) -> Res<Profile> {
    match s {
        "light" => Ok(Profile::Light),
        "standard" => Ok(Profile::Standard),
        "paranoid" => Ok(Profile::Paranoid),
        "manual" => Ok(Profile::Manual),
        other => Err(format!("неизвестный профиль `{other}`")),
    }
}

fn parse_pass_id(s: &str) -> Res<PassId> {
    [
        PassId::Rename,
        PassId::Strings,
        PassId::Constants,
        PassId::Flow,
        PassId::Noise,
        PassId::DebugInfo,
        PassId::Resources,
    ]
    .into_iter()
    .find(|id| id.as_str() == s)
    .ok_or_else(|| format!("неизвестный пасс `{s}`"))
}

fn parse_pass_id_vec(v: &Value, path: &str) -> Res<Vec<PassId>> {
    expect_string_vec(v, path)?
        .iter()
        .map(|s| parse_pass_id(s))
        .collect()
}

/// Reject any key in `table` that isn't in `known` (mirrors serde's
/// `deny_unknown_fields`).
fn check_known_keys(table: &Table, known: &[&str], section: &str) -> Res<()> {
    for key in table.keys() {
        if !known.contains(&key.as_str()) {
            return Err(unknown_key(section, key));
        }
    }
    Ok(())
}

fn sub_table<'a>(table: &'a Table, key: &str) -> Res<Option<&'a Table>> {
    match table.get(key) {
        None => Ok(None),
        Some(Value::Table(t)) => Ok(Some(t)),
        Some(_) => Err(format!("`{key}` должен быть секцией `[{key}]`")),
    }
}

/// Build a full [`Config`] from a parsed root table.
pub(super) fn build(root: &Table) -> Res<Config> {
    check_known_keys(
        root,
        &[
            "profile",
            "seed",
            "enable",
            "disable",
            "keep",
            "keep_resources",
            "rename",
            "strings",
            "constants",
            "flow",
            "noise",
            "debug",
        ],
        "<root>",
    )?;

    let mut cfg = Config::default();
    if let Some(v) = root.get("profile") {
        cfg.profile = parse_profile(expect_str(v, "profile")?)?;
    }
    if let Some(v) = root.get("seed") {
        cfg.seed = Some(expect_u64(v, "seed")?);
    }
    if let Some(v) = root.get("enable") {
        cfg.enable = parse_pass_id_vec(v, "enable")?;
    }
    if let Some(v) = root.get("disable") {
        cfg.disable = parse_pass_id_vec(v, "disable")?;
    }
    if let Some(v) = root.get("keep") {
        cfg.keep = expect_string_vec(v, "keep")?;
    }
    if let Some(v) = root.get("keep_resources") {
        cfg.keep_resources = expect_string_vec(v, "keep_resources")?;
    }
    if let Some(t) = sub_table(root, "rename")? {
        cfg.rename = sections::build_rename(t)?;
    }
    if let Some(t) = sub_table(root, "strings")? {
        cfg.strings = sections::build_strings(t)?;
    }
    if let Some(t) = sub_table(root, "constants")? {
        cfg.constants = sections::build_constants(t)?;
    }
    if let Some(t) = sub_table(root, "flow")? {
        cfg.flow = sections::build_flow(t)?;
    }
    if let Some(t) = sub_table(root, "noise")? {
        cfg.noise = sections::build_noise(t)?;
    }
    if let Some(t) = sub_table(root, "debug")? {
        cfg.debug = sections::build_debug(t)?;
    }
    Ok(cfg)
}
