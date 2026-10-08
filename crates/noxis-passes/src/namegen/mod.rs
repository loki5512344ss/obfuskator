//! Генерация имён: стили, анти-ключевые слова, уникальность в scope.

pub mod alphabet;

use std::collections::HashSet;

use noxis_core::{NameStyle, Rng};

use self::alphabet::{
    AVOID_CLASS, AVOID_MEMBER, CONFUSABLE, DECOY_LEFT, DECOY_RIGHT, KEYWORDS, LATIN_LOWER,
    LATIN_UPPER, UNICODE_LETTERS,
};

/// Генератор имён с гарантированной уникальностью в пределах одного scope.
pub struct NameGenerator {
    rng: Rng,
    style: NameStyle,
    used: HashSet<String>,
    /// Счётчик-страховка, если случайные варианты исчерпаны.
    fallback: u64,
    avoid_class: bool,
}

impl NameGenerator {
    /// Генератор для указанного стиля.
    pub fn new(rng: Rng, style: NameStyle) -> Self {
        Self {
            rng,
            style,
            used: HashSet::new(),
            fallback: 0,
            avoid_class: false,
        }
    }

    /// Режим «имена классов»: дополнительно избегаем имён из JDK/аннотаций.
    pub fn for_classes(mut self) -> Self {
        self.avoid_class = true;
        self
    }

    /// Занять конкретное имя (защита от коллизий с keep-именами).
    pub fn reserve(&mut self, name: impl Into<String>) {
        self.used.insert(name.into());
    }

    /// Следующее свободное имя.
    pub fn next_name(&mut self) -> String {
        for _ in 0..64 {
            let candidate = self.candidate();
            if self.acceptable(&candidate) {
                self.used.insert(candidate.clone());
                return candidate;
            }
        }
        // Страховка: детерминированный счётчик, гарантированно уникален.
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
            NameStyle::DecoyWords => "tmp",
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
                    // Первый символ — только буква (не цифра).
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

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]
    use super::alphabet::{AVOID_MEMBER, KEYWORDS};
    use super::*;

    #[test]
    fn names_are_unique_and_valid() {
        let mut names = NameGenerator::new(Rng::from_seed(1), NameStyle::ShortLatin);
        let mut seen = HashSet::new();
        for _ in 0..500 {
            let name = names.next_name();
            assert!(seen.insert(name.clone()), "повтор имени {name}");
            assert!(!KEYWORDS.contains(&name.as_str()));
            assert!(name.starts_with(|c: char| c.is_ascii_alphabetic()));
        }
    }

    #[test]
    fn avoids_reserved_member_names() {
        let mut names = NameGenerator::new(Rng::from_seed(2), NameStyle::DecoyWords);
        for _ in 0..500 {
            let name = names.next_name();
            assert!(!AVOID_MEMBER.contains(&name.as_str()));
        }
    }

    #[test]
    fn unicode_style_produces_identifier_valid_names() {
        let mut names = NameGenerator::new(Rng::from_seed(3), NameStyle::Unicode);
        for _ in 0..100 {
            let name = names.next_name();
            assert!(
                name.chars()
                    .all(|c| c.is_alphanumeric() || c == '_' || c == '$')
            );
        }
    }

    #[test]
    fn reserved_names_never_reissued() {
        let mut names = NameGenerator::new(Rng::from_seed(4), NameStyle::ShortLatin);
        names.reserve("abc");
        for _ in 0..300 {
            assert_ne!(names.next_name(), "abc");
        }
    }
}
