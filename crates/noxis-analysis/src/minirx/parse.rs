//! Pattern string -> flat `Node` sequence (no groups, no alternation).

use std::fmt;

/// A single class-member: a literal char or an inclusive range.
#[derive(Debug, Clone, Copy)]
pub(super) enum ClassItem {
    Char(char),
    Range(char, char),
    Space,
}

impl ClassItem {
    fn matches(&self, c: char) -> bool {
        match *self {
            ClassItem::Char(x) => x == c,
            ClassItem::Range(lo, hi) => lo <= c && c <= hi,
            ClassItem::Space => c.is_whitespace(),
        }
    }
}

/// One zero-width or single-char-consuming atom in the pattern.
#[derive(Debug, Clone)]
pub(super) enum Atom {
    Char(char),
    Any,
    Space,
    Class {
        negated: bool,
        items: Vec<ClassItem>,
    },
}

impl Atom {
    pub(super) fn matches(&self, c: char) -> bool {
        match self {
            Atom::Char(x) => *x == c,
            Atom::Any => true,
            Atom::Space => c.is_whitespace(),
            Atom::Class { negated, items } => {
                let hit = items.iter().any(|item| item.matches(c));
                hit != *negated
            }
        }
    }
}

/// A node in the flat pattern sequence.
#[derive(Debug, Clone)]
pub(super) enum Node {
    Start,
    End,
    WordBoundary,
    /// `atom` repeated `min..=max` times (`max = None` means unbounded),
    /// matched greedily with backtracking.
    Rep {
        atom: Atom,
        min: usize,
        max: Option<usize>,
    },
}

/// A pattern compilation error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub(super) String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for Error {}

/// Parses `pattern`, returning its node sequence and whether a leading
/// `(?i)` flag was present.
pub(super) fn parse(pattern: &str) -> Result<(Vec<Node>, bool), Error> {
    let (rest, case_insensitive) = match pattern.strip_prefix("(?i)") {
        Some(rest) => (rest, true),
        None => (pattern, false),
    };
    let chars: Vec<char> = if case_insensitive {
        rest.chars().flat_map(char::to_lowercase).collect()
    } else {
        rest.chars().collect()
    };
    let mut nodes = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let node = match c {
            '^' => {
                i += 1;
                Node::Start
            }
            '$' => {
                i += 1;
                Node::End
            }
            '\\' => {
                i += 1;
                let Some(&esc) = chars.get(i) else {
                    return Err(Error("trailing backslash".into()));
                };
                i += 1;
                if esc == 'b' {
                    Node::WordBoundary
                } else {
                    let atom = if esc == 's' {
                        Atom::Space
                    } else {
                        Atom::Char(esc)
                    };
                    parse_quantifier(atom, &chars, &mut i)
                }
            }
            '.' => {
                i += 1;
                parse_quantifier(Atom::Any, &chars, &mut i)
            }
            '[' => {
                i += 1;
                let atom = parse_class(&chars, &mut i)?;
                parse_quantifier(atom, &chars, &mut i)
            }
            _ => {
                i += 1;
                parse_quantifier(Atom::Char(c), &chars, &mut i)
            }
        };
        nodes.push(node);
    }
    Ok((nodes, case_insensitive))
}

/// Reads an optional trailing `*`/`+`/`?` and wraps `atom` into a `Rep`.
fn parse_quantifier(atom: Atom, chars: &[char], i: &mut usize) -> Node {
    let (min, max) = match chars.get(*i) {
        Some('*') => {
            *i += 1;
            (0, None)
        }
        Some('+') => {
            *i += 1;
            (1, None)
        }
        Some('?') => {
            *i += 1;
            (0, Some(1))
        }
        _ => (1, Some(1)),
    };
    Node::Rep { atom, min, max }
}

/// Parses `[...]` starting right after the opening `[`; advances `i` past
/// the closing `]`.
fn parse_class(chars: &[char], i: &mut usize) -> Result<Atom, Error> {
    let negated = if matches!(chars.get(*i), Some('^')) {
        *i += 1;
        true
    } else {
        false
    };
    let mut items = Vec::new();
    let mut closed = false;
    while let Some(&c) = chars.get(*i) {
        if c == ']' {
            *i += 1;
            closed = true;
            break;
        }
        let lo = if c == '\\' {
            *i += 1;
            let esc = *chars
                .get(*i)
                .ok_or_else(|| Error("trailing backslash in class".into()))?;
            *i += 1;
            if esc == 's' {
                items.push(ClassItem::Space);
                continue;
            }
            esc
        } else {
            *i += 1;
            c
        };
        if chars.get(*i) == Some(&'-') && chars.get(*i + 1).is_some_and(|&c| c != ']') {
            *i += 1;
            let hi = chars[*i];
            *i += 1;
            items.push(ClassItem::Range(lo, hi));
        } else {
            items.push(ClassItem::Char(lo));
        }
    }
    if !closed {
        return Err(Error("unterminated character class".into()));
    }
    Ok(Atom::Class { negated, items })
}
