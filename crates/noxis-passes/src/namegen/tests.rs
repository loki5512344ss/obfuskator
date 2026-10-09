//! Name generator tests: uniqueness, validity, per-style shape.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::HashSet;

use noxis_core::{NameStyle, Rng};

use super::NameGenerator;
use super::alphabet::{AVOID_MEMBER, KEYWORDS};

#[test]
fn names_are_unique_and_valid() {
    let mut names = NameGenerator::new(Rng::from_seed(1), NameStyle::ShortLatin);
    let mut seen = HashSet::new();
    for _ in 0..500 {
        let name = names.next_name();
        assert!(seen.insert(name.clone()), "duplicate name {name}");
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

#[test]
fn plausible_class_names_look_like_noun_suffix() {
    let mut names = NameGenerator::new(Rng::from_seed(5), NameStyle::Plausible).for_classes();
    for _ in 0..50 {
        let name = names.next_name();
        assert!(name.starts_with(|c: char| c.is_ascii_uppercase()), "{name}");
        // Two words run together in CamelCase with no separator.
        assert!(
            name.chars().filter(|c| c.is_ascii_uppercase()).count() >= 2,
            "{name}"
        );
    }
}

#[test]
fn plausible_member_names_look_like_verb_noun() {
    let mut names = NameGenerator::new(Rng::from_seed(6), NameStyle::Plausible);
    for _ in 0..50 {
        let name = names.next_name();
        assert!(name.starts_with(|c: char| c.is_ascii_lowercase()), "{name}");
        assert!(!KEYWORDS.contains(&name.as_str()));
    }
}
