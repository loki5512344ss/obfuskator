//! Hand-rolled parser for the small TOML subset `Config` actually uses.
//!
//! We don't need full TOML (dates, multi-line strings, inline tables,
//! arrays-of-tables): every fixture and hand-written config in this repo
//! is flat `key = value` pairs grouped by `[section]` / `[section.sub]`
//! headers, with scalars limited to strings, integers, bools and arrays
//! of those. Replacing the `toml` crate with the full spec would be far
//! more code than this project needs; this covers exactly the syntax
//! `Config::from_toml` has ever been asked to parse (see
//! `crates/noxis-core/src/config/from_toml.rs` for the struct-population
//! side), erroring out loudly on anything else instead of guessing.

use std::collections::BTreeMap;
use std::fmt;

/// A parsed scalar or compound TOML-subset value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    String(String),
    Integer(i64),
    Bool(bool),
    Array(Vec<Value>),
    Table(Table),
}

/// A `[section]` body: key → value, in the order keys were written.
pub type Table = BTreeMap<String, Value>;

/// A parse failure, with a human-readable message (no line/col tracking -
/// configs here are short enough that the offending text is enough).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError(pub String);

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

fn err(msg: impl Into<String>) -> ParseError {
    ParseError(msg.into())
}

/// Parse TOML-subset source text into a root [`Table`].
pub fn parse(text: &str) -> Result<Table, ParseError> {
    let mut root: Table = Table::new();
    let mut path: Vec<String> = Vec::new();

    for raw_line in text.lines() {
        let line = strip_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }
        if let Some(header) = line.strip_prefix('[') {
            let header = header
                .strip_suffix(']')
                .ok_or_else(|| err(format!("незакрытый заголовок секции: `{raw_line}`")))?;
            path = header
                .split('.')
                .map(|seg| seg.trim().to_string())
                .collect();
            if path.iter().any(|seg| seg.is_empty()) {
                return Err(err(format!("пустой сегмент в заголовке: `{raw_line}`")));
            }
            ensure_table_path(&mut root, &path)?;
            continue;
        }
        let eq = find_top_level_eq(line)
            .ok_or_else(|| err(format!("ожидалось `key = value`: `{raw_line}`")))?;
        let key = line[..eq].trim();
        if key.is_empty() {
            return Err(err(format!("пустой ключ: `{raw_line}`")));
        }
        let value = parse_value(line[eq + 1..].trim())?;
        let table = table_at_path(&mut root, &path)?;
        table.insert(key.to_string(), value);
    }

    Ok(root)
}

/// Drop a `#`-started comment, unless the `#` is inside a quoted string.
fn strip_comment(line: &str) -> &str {
    let mut in_string = false;
    for (i, c) in line.char_indices() {
        match c {
            '"' => in_string = !in_string,
            '#' if !in_string => return &line[..i],
            _ => {}
        }
    }
    line
}

/// Find the first `=` that isn't inside a quoted string.
fn find_top_level_eq(line: &str) -> Option<usize> {
    let mut in_string = false;
    for (i, c) in line.char_indices() {
        match c {
            '"' => in_string = !in_string,
            '=' if !in_string => return Some(i),
            _ => {}
        }
    }
    None
}

fn ensure_table_path(root: &mut Table, path: &[String]) -> Result<(), ParseError> {
    let mut cur = root;
    for seg in path {
        let entry = cur
            .entry(seg.clone())
            .or_insert_with(|| Value::Table(Table::new()));
        match entry {
            Value::Table(t) => cur = t,
            _ => return Err(err(format!("`{seg}` уже используется как не-секция"))),
        }
    }
    Ok(())
}

fn table_at_path<'a>(root: &'a mut Table, path: &[String]) -> Result<&'a mut Table, ParseError> {
    let mut cur = root;
    for seg in path {
        match cur.get_mut(seg.as_str()) {
            Some(Value::Table(t)) => cur = t,
            _ => return Err(err(format!("секция `{seg}` не найдена"))),
        }
    }
    Ok(cur)
}

fn parse_value(s: &str) -> Result<Value, ParseError> {
    let s = s.trim();
    if let Some(inner) = s.strip_prefix('"') {
        let inner = inner
            .strip_suffix('"')
            .ok_or_else(|| err(format!("незакрытая строка: `{s}`")))?;
        return Ok(Value::String(unescape(inner)?));
    }
    if let Some(inner) = s.strip_prefix('[') {
        let inner = inner
            .strip_suffix(']')
            .ok_or_else(|| err(format!("незакрытый массив: `{s}`")))?;
        return Ok(Value::Array(parse_array_elements(inner)?));
    }
    match s {
        "true" => return Ok(Value::Bool(true)),
        "false" => return Ok(Value::Bool(false)),
        _ => {}
    }
    s.parse::<i64>()
        .map(Value::Integer)
        .map_err(|_| err(format!("неподдерживаемое значение: `{s}`")))
}

fn parse_array_elements(inner: &str) -> Result<Vec<Value>, ParseError> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut in_string = false;
    let mut start = 0usize;
    let bytes = inner.as_bytes();
    let push = |out: &mut Vec<Value>, chunk: &str| -> Result<(), ParseError> {
        let chunk = chunk.trim();
        if chunk.is_empty() {
            return Ok(());
        }
        out.push(parse_value(chunk)?);
        Ok(())
    };
    for (i, c) in inner.char_indices() {
        match c {
            '"' => in_string = !in_string,
            '[' if !in_string => depth += 1,
            ']' if !in_string => depth -= 1,
            ',' if !in_string && depth == 0 => {
                push(&mut out, &inner[start..i])?;
                start = i + 1;
            }
            _ => {}
        }
    }
    if start < bytes.len() {
        push(&mut out, &inner[start..])?;
    }
    Ok(out)
}

/// Basic escapes actually seen in configs: `\"`, `\\`, `\n`, `\t`.
fn unescape(s: &str) -> Result<String, ParseError> {
    if !s.contains('\\') {
        return Ok(s.to_string());
    }
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            other => {
                return Err(err(format!(
                    "неподдерживаемый escape-символ: `\\{}`",
                    other.unwrap_or(' ')
                )));
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
