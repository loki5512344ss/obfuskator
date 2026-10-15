//! Repro for the predecessor frame bug (todo.md §3.4): recompute frames on
//! every method of an UNTOUCHED jar (no pass runs), write, verify with real
//! `java -Xverify:all`, report which classes fail and why.
//!
//! ```text
//! cargo run --release --example frame_repro -- --jar <path> [--out <path>]
//! ```
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use noxis_analysis::{ClassHierarchy, JdkHierarchy};
use noxis_classfile::{compute_frames, format};
use noxis_jar::{load, read_file, save, write_file};
use noxis_verify::verify_jar;

fn main() {
    let jar = std::env::args()
        .skip_while(|a| a != "--jar")
        .nth(1)
        .expect("--jar <path>");
    let entries = read_file(&PathBuf::from(&jar)).expect("read jar");
    let mut program = load(entries).expect("load program");

    let base = ClassHierarchy::build(&program);
    let hierarchy = JdkHierarchy::wrap(&base);

    let mut recomputed = 0usize;
    let mut failed_recompute = 0usize;
    for ci in 0..program.classes.len() {
        let class_name = program.classes[ci].this_class.clone();
        let methods = program.classes[ci].methods.len();
        for mi in 0..methods {
            let Some(snapshot) = program.classes[ci].methods[mi].code.clone() else {
                continue;
            };
            let method = program.classes[ci].methods[mi].clone();
            match compute_frames(&class_name, &method, &hierarchy) {
                Ok(frames) => {
                    program.classes[ci].methods[mi]
                        .code
                        .as_mut()
                        .expect("code")
                        .frames = frames;
                    recomputed += 1;
                }
                Err(e) => {
                    failed_recompute += 1;
                    eprintln!("compute_frames FAILED {class_name}#{} : {e}", method.name);
                    // Keep the stored frames instead of giving up the class.
                    program.classes[ci].methods[mi].code = Some(snapshot);
                }
            }
        }
    }
    println!("recomputed {recomputed} methods, {failed_recompute} compute_frames errors");

    // Round-trip every class through the writer, so a write-time bug shows
    // up the same way it would in the real pipeline.
    let mut write_failed = 0usize;
    for class in &program.classes {
        if let Err(e) = format::write(class) {
            write_failed += 1;
            eprintln!("write FAILED {}: {e}", class.this_class);
        }
    }
    println!("write errors: {write_failed}");

    let entries = save(&program).expect("save program");
    let out = std::env::args()
        .skip_while(|a| a != "--out")
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("frame_repro_out.jar"));
    write_file(&out, &entries).expect("write jar");
    println!("wrote {}", out.display());

    let report = verify_jar(&out, &[]);
    let mut pass = 0usize;
    let mut fail = 0usize;
    let mut skip = 0usize;
    for (name, verdict) in &report {
        match verdict {
            noxis_verify::JavaVerdict::Verified => pass += 1,
            noxis_verify::JavaVerdict::Failed(msg) => {
                fail += 1;
                println!("FAIL {name}: {msg}");
            }
            noxis_verify::JavaVerdict::Skipped(_) => skip += 1,
        }
    }
    println!("PASS={pass} FAIL={fail} SKIP={skip}");
}
