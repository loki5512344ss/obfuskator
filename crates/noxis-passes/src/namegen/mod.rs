//! Name generation: styles, anti-keyword checks, uniqueness within a scope.

pub mod alphabet;
#[cfg(test)]
mod tests;

use std::collections::HashSet;

use noxis_core::{NameStyle, Rng};

use self::alphabet::{
    AVOID_CLASS, AVOID_MEMBER, CONFUSABLE, DECOY_LEFT, DECOY_RIGHT, KEYWORDS, LATIN_LOWER,
    LATIN_UPPER, PLAUSIBLE_NOUN, PLAUSIBLE_SUFFIX, PLAUSIBLE_VERB, UNICODE_LETTERS,
};

/// Name generator with guaranteed uniqueness within a single scope.
pub struct NameGenerator {
    rng: Rng,
    style: NameStyle,
    used: HashSet<String>,
    /// Fallback counter for when the random pool is exhausted.
    fallback: u64,
    avoid_class: bool,
}

impl NameGenerator {
    /// Generator for the given style.
    pub fn new(rng: Rng, style: NameStyle) -> Self {
        Self {
            rng,
            style,
            used: HashSet::new(),
            fallback: 0,
            avoid_class: false,
        }
    }

    /// "Class name" mode: additionally avoid names from the JDK/annotations.
    pub fn for_classes(mut self) -> Self {
        self.avoid_class = true;
        self
    }

    /// Reserve a specific name (protects against collisions with keep-listed names).
    pub fn reserve(&mut self, name: impl Into<String>) {
        self.used.insert(name.into());
    }

    /// The next free name.
    pub fn next_name(&mut self) -> String {
        for _ in 0..64 {
            let candidate = self.candidate();
            if self.acceptable(&candidate) {
                self.used.insert(candidate.clone());
                return candidate;
            }
        }
        // Fallback: a deterministic counter, guaranteed unique.
        loop {
            self.fallback += 1;
            let candidate = format!("{}{}", self.style_prefix(), self.fallback);
            if self.acceptable(&candidate) {
                self.used.insert(candidate.clone());
                return candidate;
            }
        }
    }

    fn style_prefix(&self) -> &'static str {
        match self.style {
            NameStyle::ShortLatin | NameStyle::IlO0 | NameStyle::MixedLatin => "z",
            NameStyle::Unicode => "\u{043e}",
            NameStyle::DecoyWords | NameStyle::Plausible => "tmp",
        }
    }

    fn candidate(&mut self) -> String {
        match self.style {
            NameStyle::ShortLatin => {
                let len = 1 + self.rng.below(3) as usize;
                random_from(LATIN_LOWER, len, &mut self.rng)
            }
            NameStyle::IlO0 => {
                let len = 3 + self.rng.below(5) as usize;
                random_from(CONFUSABLE, len, &mut self.rng)
            }
            NameStyle::MixedLatin => {
                let len = 6 + self.rng.below(7) as usize;
                let mut out = String::with_capacity(len);
                for i in 0..len {
                    let alpha = if self.rng.below(2) == 0 {
                        LATIN_LOWER
                    } else {
                        LATIN_UPPER
                    };
                    let idx = self.rng.below(alpha.len() as u64) as usize;
                    let ch = alpha[idx] as char;
                    // First character must be a letter, not a digit.
                    if i == 0 && !ch.is_ascii_alphabetic() {
                        out.push('a');
                    } else {
                        out.push(ch);
                    }
                }
                out
            }
            NameStyle::Unicode => {
                let len = 4 + self.rng.below(6) as usize;
                (0..len)
                    .map(|_| {
                        let idx = self.rng.below(UNICODE_LETTERS.len() as u64) as usize;
                        UNICODE_LETTERS[idx]
                    })
                    .collect()
            }
            NameStyle::DecoyWords => {
                let l = DECOY_LEFT[self.rng.below(DECOY_LEFT.len() as u64) as usize];
                let r = DECOY_RIGHT[self.rng.below(DECOY_RIGHT.len() as u64) as usize];
                let sep = if self.rng.below(2) == 0 { "_" } else { "" };
                let mut name = format!("{l}{sep}{r}");
                if self.rng.below(2) == 0 {
                    name = capitalize(&name);
                }
                name
            }
            // Blends in as ordinary application code instead of obvious
            // noise: classes get `NounSuffix` (`TextureCache`), members get
            // camelCase `verbNoun` (`resolveToken`).
            NameStyle::Plausible => {
                let noun = PLAUSIBLE_NOUN[self.rng.below(PLAUSIBLE_NOUN.len() as u64) as usize];
                if self.avoid_class {
                    let suffix =
                        PLAUSIBLE_SUFFIX[self.rng.below(PLAUSIBLE_SUFFIX.len() as u64) as usize];
                    format!("{noun}{suffix}")
                } else {
                    let verb = PLAUSIBLE_VERB[self.rng.below(PLAUSIBLE_VERB.len() as u64) as usize];
                    format!("{verb}{noun}")
                }
            }
        }
    }

    fn acceptable(&self, name: &str) -> bool {
        if name.is_empty() || self.used.contains(name) {
            return false;
        }
        if KEYWORDS.contains(&name) {
            return false;
        }
        if self.avoid_class && AVOID_CLASS.contains(&name) {
            return false;
        }
        if !self.avoid_class && AVOID_MEMBER.contains(&name) {
            return false;
        }
        let Some(first) = name.chars().next() else {
            return false;
        };
        if !first.is_alphabetic() && first != '_' && first != '$' {
            return false;
        }
        name.chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '$')
    }
}

fn random_from(alphabet: &[u8], len: usize, rng: &mut Rng) -> String {
    (0..len)
        .map(|_| {
            let idx = rng.below(alphabet.len() as u64) as usize;
            alphabet[idx] as char
        })
        .collect()
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}
