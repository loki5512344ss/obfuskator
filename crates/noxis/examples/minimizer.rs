//! Minimizer: isolate a `VerifyError` to the smallest failing method set.
//!
//! The full pipeline runs on the whole jar (so hierarchy and the classpath
//! are complete), then the transformed target is verified with the whole
//! transformed jar behind it. Bisection deletes methods from the target
//! only: the verifier rejects a class method-locally, so the smallest subset
//! that still fails names the culprit.
//!
//! ```text
//! cargo run --example minimizer -- \
//!     --jar mod.jar --class a/b/C [--seed 42] [--config p.toml] [--bisect]
//! ```
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use noxis_classfile::format;
use noxis_classfile::model::ClassFile;
use noxis_core::{Config, Context, PassId, Profile};
use noxis_jar::{Entry, load, read_file, save, write_file};

fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("minimizer: {error}");
            ExitCode::from(2)
        }
    }
}

struct Args {
    jar: String,
    class: String,
    seed: u64,
    config: Option<String>,
    bisect: bool,
    recompute: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        jar: String::new(),
        class: String::new(),
        seed: 1,
        config: None,
        bisect: false,
        recompute: false,
    };
    let mut iter = std::env::args().skip(1);
    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--jar" => args.jar = iter.next().ok_or("--jar needs a value")?,
            "--class" => args.class = iter.next().ok_or("--class needs a value")?,
            "--seed" => {
                args.seed = iter
                    .next()
                    .ok_or("--seed needs a value")?
                    .parse()
                    .map_err(|_| "--seed must be a number")?
            }
            "--config" => args.config = Some(iter.next().ok_or("--config needs a value")?),
            "--bisect" => args.bisect = true,
            "--recompute" => args.recompute = true,
            other => return Err(format!("unknown flag {other}")),
        }
    }
    if args.jar.is_empty() || args.class.is_empty() {
        return Err(
            "usage: --jar <jar> --class <internal/name> [--seed N] [--config p.toml] [--bisect]"
                .into(),
        );
    }
    Ok(args)
}

fn load_config(args: &Args) -> Result<Config, String> {
    let mut config = match &args.config {
        Some(path) => {
            let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
            Config::from_toml(&text).map_err(|e| e.to_string())?
        }
        None => Config {
            profile: Profile::Paranoid,
            ..Config::default()
        },
    };
    config.seed = Some(args.seed);
    // Junk classes never touch existing frames and only clutter the list.
    config.disable.push(PassId::Noise);
    Ok(config)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Verdict {
    Verified,
    Failed,
    Skipped,
}

/// Verify a class with the full transformed jar behind it on the classpath.
fn verify(classpath: &Path, name: &str, bytes: &[u8]) -> (Verdict, String) {
    let scratch = std::env::temp_dir().join(format!("noxis-min-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    let mut path = scratch.clone();
    for part in name.split('/') {
        path.push(part);
    }
    path.set_extension("class");
    let written =
        std::fs::create_dir_all(path.parent().unwrap()).and_then(|()| std::fs::write(&path, bytes));
    if let Err(error) = written {
        return (Verdict::Skipped, format!("write: {error}"));
    }
    // scratch first so the (subset) target shadows its full-jar entry.
    let cp = format!("{}:{}", scratch.display(), classpath.display());
    let output = Command::new("java")
        .arg("-Xverify:all")
        .arg("-cp")
        .arg(&cp)
        .arg(name.replace('/', "."))
        .output();
    let _ = std::fs::remove_dir_all(&scratch);
    let Ok(output) = output else {
        return (Verdict::Skipped, "java failed to start".into());
    };
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if stderr.contains("VerifyError") || stderr.contains("ClassFormatError") {
        (Verdict::Failed, stderr)
    } else if stderr.contains("NoClassDefFoundError")
        || stderr.contains("ClassNotFoundException")
        || stderr.contains("UnsupportedClassVersionError")
    {
        (Verdict::Skipped, stderr)
    } else {
        (Verdict::Verified, stderr)
    }
}

/// Extract the failing class and method from a VerifyError's `Location:` line:
/// `    pkg/Class.method(desc)R @pc: insn`. The verifier blames the exact
/// method, which may live in a superclass of the class under test. Note the
/// class name is printed verbatim (an obfuscated name may itself start
/// with `L`), so both spellings are tried by the caller.
fn failing_location(stderr: &str) -> Option<(String, String)> {
    let mut lines = stderr.lines();
    while let Some(line) = lines.next() {
        if line.trim() == "Location:" {
            let loc = lines.next()?.trim();
            let dot = loc.find('.')?;
            let class = loc[..dot].to_string();
            let method = loc[dot + 1..].split(" @").next()?.to_string();
            return Some((class, method));
        }
    }
    None
}

/// Method identity as the verifier sees it: name plus descriptor. All
/// methods participate, including `<init>`/`<clinit>`: deletion is
/// verification-safe (no code runs), and the bug may live there.
fn method_keys(class: &ClassFile) -> Vec<(String, String)> {
    class
        .methods
        .iter()
        .map(|m| (m.name.clone(), m.descriptor.clone()))
        .collect()
}

fn write_subset(class: &ClassFile, keep: &BTreeSet<(String, String)>) -> Option<Vec<u8>> {
    let mut cut = class.clone();
    cut.methods
        .retain(|m| keep.contains(&(m.name.clone(), m.descriptor.clone())));
    format::write(&cut).ok()
}

fn subset_verdict(cp: &Path, class: &ClassFile, keep: &BTreeSet<(String, String)>) -> Verdict {
    let Some(bytes) = write_subset(class, keep) else {
        return Verdict::Skipped;
    };
    let (verdict, stderr) = verify(cp, &class.this_class, &bytes);
    if verdict == Verdict::Failed {
        let reason = stderr
            .lines()
            .find(|l| l.contains("VerifyError"))
            .unwrap_or("?");
        println!("  keep={}: {reason}", keep.len());
    }
    verdict
}

fn bisect(cp: &Path, class: &ClassFile) -> Option<BTreeSet<(String, String)>> {
    let mut keep: BTreeSet<(String, String)> = method_keys(class).into_iter().collect();
    println!("bisect: {} methods total", keep.len());
    while keep.len() > 1 {
        let all: Vec<(String, String)> = keep.iter().cloned().collect();
        let half = all.len() / 2;
        let left: BTreeSet<_> = all[..half].iter().cloned().collect();
        let right: BTreeSet<_> = all[half..].iter().cloned().collect();
        match (
            subset_verdict(cp, class, &left),
            subset_verdict(cp, class, &right),
        ) {
            (Verdict::Failed, _) => keep = left,
            (_, Verdict::Failed) => keep = right,
            _ => {
                println!("bisect: neither half fails alone - interaction; keeping all");
                return Some(keep);
            }
        }
    }
    Some(keep)
}

/// Recompute frames for a method as written: proves whether the stored
/// table is stale (code grew after compute) or compute itself truncates.
fn recompute(jar_path: &Path, target: &str) -> Result<bool, String> {
    use noxis_analysis::{ClassHierarchy, JdkHierarchy};
    use noxis_classfile::compute_frames;

    let entries = read_file(jar_path).map_err(|e| e.to_string())?;
    let program = load(entries).map_err(|e| e.to_string())?;
    let (class_name, method_name) = target
        .rsplit_once('#')
        .ok_or("usage: --recompute --class <internal/name>#<method>")?;
    let class = program
        .class(class_name)
        .ok_or_else(|| format!("{class_name} not in jar"))?;
    let method = class
        .methods
        .iter()
        .find(|m| m.name == method_name)
        .ok_or_else(|| format!("{method_name} not in {class_name}"))?;
    let base = ClassHierarchy::build(&program);
    let hierarchy = JdkHierarchy::wrap(&base);
    let fresh =
        compute_frames(class_name, method, &hierarchy).map_err(|e| format!("compute: {e}"))?;
    let stored = method.code.as_ref().map_or(0, |c| c.frames.len());
    println!("stored frames: {stored}, recomputed: {}", fresh.len());
    // Last covered bci of each table.
    for (label, frames) in [
        (
            "stored",
            method.code.as_ref().map_or(&[][..], |c| &c.frames[..]),
        ),
        ("fresh", &fresh[..]),
    ] {
        let mut bci: i64 = -1;
        for frame in frames {
            let d = match frame {
                noxis_classfile::StackMapFrame::Same { offset_delta }
                | noxis_classfile::StackMapFrame::SameLocals { offset_delta, .. }
                | noxis_classfile::StackMapFrame::Chop { offset_delta, .. }
                | noxis_classfile::StackMapFrame::Append { offset_delta, .. }
                | noxis_classfile::StackMapFrame::Full { offset_delta, .. } => *offset_delta as i64,
            };
            bci = if bci < 0 { d } else { bci + d + 1 };
        }
        println!("{label}: last bci {bci}");
    }
    Ok(false)
}

fn run() -> Result<bool, String> {
    let args = parse_args()?;
    let jar_path = PathBuf::from(&args.jar);
    if args.recompute {
        return recompute(&jar_path, &args.class);
    }
    let config = load_config(&args)?;
    let entries = read_file(&jar_path).map_err(|e| e.to_string())?;
    if !entries
        .iter()
        .any(|e| e.name == format!("{}.class", args.class))
    {
        return Err(format!("{} not in jar", args.class));
    }
    let mut program = load(entries).map_err(|e| e.to_string())?;
    println!("loaded {} classes", program.classes.len());

    let mut ctx = Context::new(config);
    noxis_passes::standard_pipeline()
        .run(&mut ctx, &mut program)
        .map_err(|e| e.to_string())?;

    let new_name = ctx
        .mapping
        .class(&args.class)
        .unwrap_or(&args.class)
        .to_string();
    println!("target {} -> {new_name}", args.class);

    // Whole transformed jar becomes the verifier's classpath.
    let out_jar = std::env::temp_dir().join(format!("noxis-min-cp-{}.jar", std::process::id()));
    let transformed: Vec<Entry> = save(&program).map_err(|e| e.to_string())?;
    write_file(&out_jar, &transformed).map_err(|e| e.to_string())?;

    let target = program
        .class(&new_name)
        .ok_or_else(|| format!("{new_name} vanished"))?;
    let bytes = format::write(target).map_err(|e| e.to_string())?;
    let (verdict, stderr) = verify(&out_jar, &new_name, &bytes);
    match verdict {
        Verdict::Verified => {
            println!("PASS: this run does not reproduce the failure");
            let _ = std::fs::remove_file(&out_jar);
            return Ok(false);
        }
        Verdict::Skipped => {
            println!("SKIP: dependencies unresolved:\n{stderr}");
            let _ = std::fs::remove_file(&out_jar);
            return Ok(false);
        }
        Verdict::Failed => {
            println!(
                "reproduced:\n{}",
                stderr.lines().take(6).collect::<Vec<_>>().join("\n")
            );
        }
    }

    let parsed = format::parse(&bytes).map_err(|e| e.to_string())?;
    if !args.bisect {
        let _ = std::fs::remove_file(&out_jar);
        return Ok(false);
    }
    // The verifier blames the exact method, which may live in a superclass:
    // pivot the bisection to the class that actually fails.
    let mut pivot_name = new_name.clone();
    let mut pivot = parsed;
    if let Some((fail_class, fail_method)) = failing_location(&stderr) {
        println!("verifier blames {fail_class}.{fail_method}");
        if fail_class != new_name {
            // The name may itself start with `L`; HotSpot prints it verbatim.
            let real_bytes = if let Some(real) = program.class(&fail_class) {
                format::write(real).map_err(|e| e.to_string())?
            } else if let Some(stripped) =
                fail_class.strip_prefix('L').and_then(|s| program.class(s))
            {
                format::write(stripped).map_err(|e| e.to_string())?
            } else {
                return Err(format!("{fail_class} not in program"));
            };
            pivot = format::parse(&real_bytes).map_err(|e| e.to_string())?;
            pivot_name = fail_class;
            println!("pivoted to {pivot_name}");
        }
    }
    let minimal = bisect(&out_jar, &pivot);
    let _ = std::fs::remove_file(&out_jar);
    let Some(minimal) = minimal else {
        return Ok(false);
    };
    println!("\nminimal failing set in {pivot_name}:");
    for (name, descriptor) in &minimal {
        let method = pivot
            .methods
            .iter()
            .find(|m| &m.name == name && &m.descriptor == descriptor)
            .unwrap();
        let insns = method.code.as_ref().map_or(0, |c| c.insns.len());
        let frames = method.code.as_ref().map_or(0, |c| c.frames.len());
        println!("  {name}{descriptor}  insns={insns} frames={frames}");
    }
    // Write the minimal single-failing-subset class for javap inspection.
    let minimal_set: BTreeSet<(String, String)> = minimal.iter().cloned().collect();
    if let Some(minimal_bytes) = write_subset(&pivot, &minimal_set) {
        let dump = std::env::temp_dir().join("noxis-minimal-fail.class");
        if std::fs::write(&dump, &minimal_bytes).is_ok() {
            println!("minimal class bytes: {}", dump.display());
        }
    }
    Ok(false)
}
