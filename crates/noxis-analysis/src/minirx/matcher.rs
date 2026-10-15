//! Backtracking matcher over a flat `Node` sequence.

use super::parse::Node;

/// True if `nodes` matches anywhere in `text` (unanchored search), honoring
/// any `Start`/`End` nodes as zero-width anchors at their actual position.
pub(super) fn search(nodes: &[Node], text: &[char]) -> bool {
    // `^` only makes sense anchored at position 0: if the pattern starts
    // with it, trying every start offset is still correct (it will simply
    // fail at every offset except possibly 0) and keeps this function
    // simple - no need to special-case it.
    for start in 0..=text.len() {
        if match_here(nodes, 0, text, start).is_some() {
            return true;
        }
    }
    false
}

/// Tries to match `nodes[idx..]` starting at `text[pos..]`; returns the end
/// position on success.
fn match_here(nodes: &[Node], idx: usize, text: &[char], pos: usize) -> Option<usize> {
    let Some(node) = nodes.get(idx) else {
        return Some(pos);
    };
    match node {
        Node::Start => {
            if pos != 0 {
                return None;
            }
            match_here(nodes, idx + 1, text, pos)
        }
        Node::End => {
            if pos != text.len() {
                return None;
            }
            match_here(nodes, idx + 1, text, pos)
        }
        Node::WordBoundary => {
            if !is_word_boundary(text, pos) {
                return None;
            }
            match_here(nodes, idx + 1, text, pos)
        }
        Node::Rep { atom, min, max } => {
            let run = count_run(atom, text, pos);
            let hi = max.map_or(run, |m| m.min(run));
            if hi < *min {
                return None;
            }
            // Greedy: try the longest repetition first, backtrack shorter.
            for count in (*min..=hi).rev() {
                if let Some(end) = match_here(nodes, idx + 1, text, pos + count) {
                    return Some(end);
                }
            }
            None
        }
    }
}

/// Counts how many consecutive chars from `pos` satisfy `atom`.
fn count_run(atom: &super::parse::Atom, text: &[char], pos: usize) -> usize {
    let mut n = 0;
    while text.get(pos + n).is_some_and(|&c| atom.matches(c)) {
        n += 1;
    }
    n
}

/// A JVM/identifier-ish "word" char: alphanumeric or `_`.
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn is_word_boundary(text: &[char], pos: usize) -> bool {
    let before = pos.checked_sub(1).and_then(|i| text.get(i)).copied();
    let after = text.get(pos).copied();
    let before_word = before.is_some_and(is_word_char);
    let after_word = after.is_some_and(is_word_char);
    before_word != after_word
}
