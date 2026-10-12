//! Пасс ресурсов: патч текстовых файлов архива по маппингу классов.
//!
//! Без этого обфускация Fabric-мода просто не стартует: `fabric.mod.json`
//! хранит main-class точкой, `*.mixins.json` - пути внутренними именами,
//! refmap и access widener - тоже. Материал: `info/evo-weaknesses.md` §3/§8
//! (у разобранного образца ресурсы вообще не трогали).
//!
//! Замена **границная**, а не подстрочная: `org/example/Foo` не должен
//! совпасть внутри `myorg/example/FooBar`. Правило то же, что у проверенного
//! референса: сначала дескрипторный вид `Lpkg/Class;`, затем внутренний вид
//! с `/`, затем бинарный с `.`, длинные имена первыми.

/// Интеграционные тесты пасса.
#[cfg(test)]
mod tests;

mod mixins;

use std::cmp::Reverse;
use std::collections::BTreeMap;

use noxis_classfile::Program;
use noxis_core::{Context, CoreError, Pass, PassId, PassReport};

pub use mixins::{patch_mixins_json, patch_refmap_json};

/// Расширения текстовых ресурсов, которые имеют смысл переписывать.
const TEXT_EXTS: [&str; 9] = [
    ".json",
    ".mcmeta",
    ".properties",
    ".txt",
    ".xml",
    ".yml",
    ".yaml",
    ".cfg",
    ".accesswidener",
];

/// Service loader'ы: файла без расширения, но каждая строка - имя класса.
const SERVICES_PREFIX: &str = "META-INF/services/";

/// Пасс патча ресурсов.
#[derive(Debug, Default, Clone, Copy)]
pub struct ResourcesPass;

impl Pass for ResourcesPass {
    fn id(&self) -> PassId {
        PassId::Resources
    }

    fn apply(&self, ctx: &mut Context, program: &mut Program) -> Result<PassReport, CoreError> {
        let map = ctx.mapping.classes.clone();
        if map.is_empty() {
            return Ok(PassReport {
                pass: PassId::Resources,
                transformed: 0,
                skipped: program.resources.len(),
                notes: vec!["маппинг классов пуст - ресурсы не тронуты".into()],
            });
        }

        let mut patched = 0usize;
        let mut skipped = 0usize;

        for resource in &mut program.resources {
            if !is_text(&resource.path) {
                skipped += 1;
                continue;
            }
            let Ok(text) = std::str::from_utf8(&resource.data) else {
                skipped += 1;
                continue;
            };
            let lower = resource.path.to_ascii_lowercase();
            let updated = if lower.ends_with(".mixins.json") || lower.ends_with("mixins.json") {
                patch_mixins_json(text, &map).unwrap_or_else(|| remap_text(text, &map))
            } else if lower.ends_with("refmap.json") {
                patch_refmap_json(text, &map).unwrap_or_else(|| remap_text(text, &map))
            } else {
                remap_text(text, &map)
            };
            if updated == text {
                skipped += 1;
                continue;
            }
            resource.data = updated.into_bytes();
            patched += 1;
        }

        let mut notes = Vec::new();
        if patched > 0 {
            notes.push(format!("патч ресурсов: {patched}, пропущено: {skipped}"));
        }

        Ok(PassReport {
            pass: PassId::Resources,
            transformed: patched,
            skipped,
            notes,
        })
    }
}

/// Текстовый ли это ресурс.
///
/// Бинарии (картинки, `.class` в `versions/`, нативные библиотеки) не
/// трогаем: их переписывание ломает содержимое молча.
pub fn is_text(path: &str) -> bool {
    if path.starts_with(SERVICES_PREFIX) {
        return !path.ends_with('/');
    }
    let lower = path.to_ascii_lowercase();
    TEXT_EXTS.iter().any(|ext| lower.ends_with(ext))
}

/// Переписывает все ссылки на классы в тексте по таблице `old → new`.
pub fn remap_text(text: &str, map: &BTreeMap<String, String>) -> String {
    if map.is_empty() {
        return text.to_string();
    }

    // Длинные имена первыми: иначе `org/example/Foo` совпадёт внутри
    // `org/example/FooBar` ещё до того, как до него дойдёт очередь.
    let mut pairs: Vec<(&String, &String)> = map.iter().collect();
    pairs.sort_by_key(|(from, _)| Reverse(from.len()));

    let mut out = text.to_string();
    for (from, to) in pairs {
        out = out.replace(&format!("L{from};"), &format!("L{to};"));
        out = replace_delimited(&out, from, to, false);
        out = replace_delimited(&out, &from.replace('/', "."), &to.replace('/', "."), true);
    }
    out
}

/// Заменяет все вхождения `old`, отделённые от surrounding текста границами
/// имени. `dotted` выбирает набор запрещённых соседей: `/` или `.`.
fn replace_delimited(text: &str, old: &str, new: &str, dotted: bool) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;

    while let Some(at) = rest.find(old) {
        let before = &rest[..at];
        let after = &rest[at + old.len()..];

        if is_boundary(before.chars().next_back(), dotted)
            && is_boundary(after.chars().next(), dotted)
        {
            out.push_str(before);
            out.push_str(new);
            rest = after;
            continue;
        }

        // Совпадение частичное - пропускаем первый символ и ищем дальше.
        let mut chars = rest[at..].chars();
        out.push_str(before);
        match chars.next() {
            Some(first) => {
                out.push(first);
                rest = chars.as_str();
            }
            None => break,
        }
    }

    out.push_str(rest);
    out
}

/// `true`, если символ слева/справа допускает начало/конец имени класса.
fn is_boundary(neighbour: Option<char>, dotted: bool) -> bool {
    let Some(ch) = neighbour else {
        return true;
    };
    if ch.is_alphanumeric() || ch == '_' || ch == '$' {
        return false;
    }
    if dotted { ch != '.' } else { ch != '/' }
}
