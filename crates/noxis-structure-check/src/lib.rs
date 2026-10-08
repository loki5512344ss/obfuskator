//! Compile-time gate for the project structure rules.
//!
//! Every crate calls [`check`] from its `build.rs`, so a file that grows past
//! 250 lines or a directory that fills up with a fifth file fails the build.

mod rules;

pub use rules::{MAX_DIR_FILES, MAX_FILE_LINES, check, scan};

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]
    use std::fs;
    use std::path::{Path, PathBuf};

    use super::scan;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("noxis-structure-{tag}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).expect("create temp dir");
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn violations(dir: &Path) -> Vec<String> {
        let mut out = Vec::new();
        scan(dir, &mut out);
        out
    }

    #[test]
    fn rejects_file_over_line_limit() {
        let tmp = TempDir::new("long");
        fs::write(tmp.0.join("lib.rs"), "fn main() {}\n".repeat(300)).expect("write");
        let found = violations(&tmp.0);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].contains("lines >"), "{found:?}");
    }

    #[test]
    fn rejects_directory_over_file_limit() {
        let tmp = TempDir::new("many");
        for index in 0..super::MAX_DIR_FILES + 1 {
            fs::write(tmp.0.join(format!("m{index}.rs")), "").expect("write");
        }
        let found = violations(&tmp.0);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].contains("files >"), "{found:?}");
    }

    #[test]
    fn accepts_compact_tree() {
        let tmp = TempDir::new("ok");
        fs::create_dir(tmp.0.join("sub")).expect("mkdir");
        fs::write(tmp.0.join("lib.rs"), "pub fn f() {}\n").expect("write");
        fs::write(tmp.0.join("sub").join("a.rs"), "pub fn a() {}\n").expect("write");
        let found = violations(&tmp.0);
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn ignores_non_limited_extensions() {
        let tmp = TempDir::new("ext");
        fs::write(tmp.0.join("notes.txt"), "line\n".repeat(300)).expect("write");
        let found = violations(&tmp.0);
        assert!(found.is_empty(), "{found:?}");
    }
}
