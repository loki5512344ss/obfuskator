//! Project structure rules, invoked from every crate's `build.rs`.
//!
//! Rules apply to Rust code only, not to documentation:
//! * every `.rs` file is at most [`MAX_FILE_LINES`] lines long;
//! * every directory inside a crate's `src/`, `tests/` or `benches/` holds
//!   at most [`MAX_DIR_FILES`] files — subdirectories are unlimited.
//!
//! Deliberately excluded: `Cargo.lock`, `.gitignore`, `docs/`, the legacy
//! `info/` tree, and crate roots (they always own `Cargo.toml` plus
//! `build.rs`).

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

/// Maximum number of lines in a Rust source file.
pub const MAX_FILE_LINES: usize = 250;

/// Maximum number of files per directory; subdirectories are not counted.
pub const MAX_DIR_FILES: usize = 4;

/// Extensions subject to [`MAX_FILE_LINES`].
const LINE_LIMITED_EXTS: [&str; 1] = ["rs"];

/// Directory names the scan never descends into.
const SKIPPED_DIRS: [&str; 2] = ["target", ".git"];

/// Scans the calling crate's `src/`/`tests/`/`benches/` and panics with a
/// report on the first violation.
///
/// # Panics
///
/// Panics whenever a rule is violated — that is the point of the gate.
pub fn check() {
    let manifest = manifest_dir();
    let mut violations = Vec::new();

    // `src/` обязателен; `tests/` и `benches/` проверяются, если созданы —
    // иначе правило «250 строк» обходится выносом кода в тесты. `docs/` вне
    // гейта: правила — только про Rust-код.
    let roots = [
        manifest.join("src"),
        manifest.join("tests"),
        manifest.join("benches"),
    ];

    for root in &roots {
        watch(root);
        scan(root, &mut violations);
    }

    if violations.is_empty() {
        return;
    }

    let mut report = String::from("project structure rules violated:\n");
    for violation in &violations {
        report.push_str("  - ");
        report.push_str(violation);
        report.push('\n');
    }
    report.push_str("split the file, or move code into a subdirectory (rules: docs/README.md)");
    panic!("{report}");
}

/// Re-runs the build script whenever a checked tree changes.
///
/// Without an explicit `rerun-if-changed`, cargo falls back to its default
/// (rerun on any change anywhere in the package); naming the trees keeps the
/// gate from firing on unrelated edits.
fn watch(root: &Path) {
    if root.is_dir() {
        println!("cargo:rerun-if-changed={}", root.display());
    }
}

/// Collects every rule violation found under `root` into `violations`.
pub fn scan(root: &Path, violations: &mut Vec<String>) {
    if !root.is_dir() {
        return;
    }
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(err) => {
            violations.push(format!("{}: unreadable: {err}", root.display()));
            return;
        }
    };

    let mut files = 0usize;
    let mut subdirs = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name();
            if SKIPPED_DIRS
                .iter()
                .any(|s| name.as_os_str() == OsStr::new(s))
            {
                continue;
            }
            subdirs.push(path);
            continue;
        }
        let ext = path.extension().and_then(OsStr::to_str).unwrap_or_default();
        if !LINE_LIMITED_EXTS.contains(&ext) {
            continue;
        }
        files += 1;
        check_line_limit(&path, violations);
    }

    if files > MAX_DIR_FILES {
        violations.push(format!(
            "{}: {files} files > {MAX_DIR_FILES} (subdirectories are allowed)",
            root.display()
        ));
    }

    for subdir in subdirs {
        scan(&subdir, violations);
    }
}

fn check_line_limit(path: &Path, violations: &mut Vec<String>) {
    let lines = count_lines(path);
    if lines > MAX_FILE_LINES {
        violations.push(format!(
            "{}: {lines} lines > {MAX_FILE_LINES}",
            path.display()
        ));
    }
}

/// Counts lines without requiring the file to be valid UTF-8.
fn count_lines(path: &Path) -> usize {
    let Ok(bytes) = fs::read(path) else {
        return 0;
    };
    if bytes.is_empty() {
        return 0;
    }
    let newlines = bytes.iter().filter(|&&b| b == b'\n').count();
    newlines + usize::from(bytes.last() != Some(&b'\n'))
}

fn manifest_dir() -> PathBuf {
    match std::env::var_os("CARGO_MANIFEST_DIR") {
        Some(value) => PathBuf::from(value),
        None => panic!("CARGO_MANIFEST_DIR is unset; run this check from a build script"),
    }
}
