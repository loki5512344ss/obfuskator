//! Minimal std-only regex engine, covering exactly the pattern vocabulary
//! used in this codebase (keep-rule globs compiled to regex, plus the
//! fixed `RED_PATTERNS` secret-scan list in `noxis-verify`).
//!
//! Not a general-purpose regex engine: no groups, no alternation, no
//! backreferences, no `{n,m}` quantifiers. What it does support:
//! literal chars, `.`, character classes (`[...]`, `[^...]`, ranges),
//! `\s`, `\b` (word boundary), anchors `^`/`$`, quantifiers `*`/`+`/`?`
//! (greedy, backtracking), a leading `(?i)` case-insensitive flag, and
//! `\`-escaping of any non-alphanumeric char (what `escape` produces).
//!
//! Split into `parse` (pattern string -> `Node` sequence) and `matcher`
//! (backtracking match over a char slice) to stay under the structure
//! gate's per-file line limit.

mod matcher;
mod parse;

pub use parse::Error;

/// A compiled pattern.
#[derive(Debug, Clone)]
pub struct Regex {
    nodes: Vec<parse::Node>,
    case_insensitive: bool,
}

impl Regex {
    /// Compiles `pattern`. See the module docs for the supported vocabulary.
    pub fn new(pattern: &str) -> Result<Self, Error> {
        let (nodes, case_insensitive) = parse::parse(pattern)?;
        Ok(Self {
            nodes,
            case_insensitive,
        })
    }

    /// Whether `text` matches anywhere (unanchored, unless the pattern
    /// itself carries `^`/`$`) - mirrors `regex::Regex::is_match`.
    pub fn is_match(&self, text: &str) -> bool {
        let chars: Vec<char> = if self.case_insensitive {
            text.chars().flat_map(char::to_lowercase).collect()
        } else {
            text.chars().collect()
        };
        matcher::search(&self.nodes, &chars)
    }
}

/// Escapes every char that could be special in a pattern, so the result
/// matches `s` literally when embedded in a larger pattern.
///
/// Mirrors the subset of `regex::escape` this codebase actually relies on:
/// everything outside `[A-Za-z0-9_]` is escaped (harmless over-escaping for
/// chars this engine treats as plain literals anyway).
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for c in s.chars() {
        if !c.is_alphanumeric() && c != '_' {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn literal_and_anchors() {
        let re = Regex::new("^abc$").unwrap();
        assert!(re.is_match("abc"));
        assert!(!re.is_match("abcd"));
        assert!(!re.is_match("xabc"));
    }

    #[test]
    fn glob_star_and_doublestar() {
        // Mirrors `keep::compile` output for `com.foo.*`.
        let re = Regex::new("^com/foo/[^/]*$").unwrap();
        assert!(re.is_match("com/foo/Bar"));
        assert!(!re.is_match("com/foo/Bar/Baz"));

        let re2 = Regex::new("^com/.*$").unwrap();
        assert!(re2.is_match("com/foo/Bar/Baz"));
    }

    #[test]
    fn question_mark_class() {
        let re = Regex::new("^a[^/]b$").unwrap();
        assert!(re.is_match("axb"));
        assert!(!re.is_match("a/b"));
    }

    #[test]
    fn escaped_literal_dot() {
        let pat = format!("^{}$", escape("a.b"));
        let re = Regex::new(&pat).unwrap();
        assert!(re.is_match("a.b"));
        assert!(!re.is_match("axb"));
    }

    #[test]
    fn red_patterns_compile_and_match() {
        let re = Regex::new(r"(?i)password\s*=\s*[^\s;]+").unwrap();
        assert!(re.is_match("password=hunter2"));
        assert!(re.is_match("PASSWORD = hunter2"));
        assert!(!re.is_match("not a secret"));

        let re2 = Regex::new(r"(?i)\bapi[_-]?key\b").unwrap();
        assert!(re2.is_match("api_key=abc"));
        assert!(re2.is_match("API-KEY: x"));
        assert!(!re2.is_match("recapitalized"));

        let re3 = Regex::new(r#"jdbc:[a-z]+://[^\s"']+"#).unwrap();
        assert!(re3.is_match("jdbc:mysql://host/db"));

        let re4 = Regex::new(r"-----BEGIN [A-Z ]*PRIVATE KEY-----").unwrap();
        assert!(re4.is_match("-----BEGIN RSA PRIVATE KEY-----"));

        let re5 = Regex::new(r#"https?://[^\s"']+"#).unwrap();
        assert!(re5.is_match("https://example.com/hook"));
        assert!(re5.is_match("http://example.com"));
    }

    #[test]
    fn clean_text_passes() {
        let re = Regex::new(r"(?i)password\s*=\s*[^\s;]+").unwrap();
        assert!(!re.is_match("hello world"));
    }
}
