//! JDK superclass fallback for frame merging.
//!
//! [`ClassHierarchy`] only knows program classes - deliberately so: rename
//! safety must stay conservative outside the program. Frame merging has the
//! opposite need: joins over JDK types (`Reader`/`BufferedReader`, a
//! multi-catch over `NoSuchMethodException`/`SecurityException`) degrade to
//! `Object` without the true superclass chain, and the verifier rejects the
//! poisoned frame at the first typed use (`putfield Reader`, a `Throwable`
//! parameter). This wrapper answers program classes from the built
//! hierarchy and asks the running JDK about the rest: one `java` process
//! (single-file launch, no `javac` needed), a line protocol, answers cached.
//! Without `java` every answer is `None` - exactly the old behaviour.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use noxis_classfile::Hierarchy;

use super::hierarchy::ClassHierarchy;

const HELPER: &str = include_str!("JdkHierarchy.java");

/// Frame-time hierarchy: program first, running JDK second.
pub struct JdkHierarchy<'a> {
    inner: &'a ClassHierarchy,
    shared: RefCell<Shared>,
}

#[derive(Default)]
struct Shared {
    cache: HashMap<String, Option<String>>,
    child: Option<Pipes>,
    dead: bool,
}

struct Pipes {
    rx: BufReader<ChildStdout>,
    tx: BufWriter<ChildStdin>,
    proc: Child,
}

impl<'a> JdkHierarchy<'a> {
    /// Wrap a built program hierarchy (spawns nothing yet).
    pub fn wrap(inner: &'a ClassHierarchy) -> Self {
        Self {
            inner,
            shared: RefCell::new(Shared::default()),
        }
    }

    fn jdk_superclass(&self, name: &str) -> Option<String> {
        let mut shared = self.shared.borrow_mut();
        if let Some(cached) = shared.cache.get(name) {
            return cached.clone();
        }
        if shared.dead {
            return None;
        }
        if shared.child.is_none() {
            match spawn_helper() {
                Some(pipes) => shared.child = Some(pipes),
                None => {
                    shared.dead = true;
                    return None;
                }
            }
        }
        let answer = match query(shared.child.as_mut(), name) {
            // Transport broke: stop spawning, keep the old behaviour.
            None => {
                shared.dead = true;
                return None;
            }
            Some(answer) => answer,
        };
        shared.cache.insert(name.to_string(), answer.clone());
        answer
    }
}

impl Hierarchy for JdkHierarchy<'_> {
    fn superclass(&self, class: &str) -> Option<String> {
        match self.inner.superclass(class) {
            Some(known) => Some(known),
            None => self.jdk_superclass(class),
        }
    }
}

impl Drop for JdkHierarchy<'_> {
    fn drop(&mut self) {
        if let Ok(mut shared) = self.shared.try_borrow_mut()
            && let Some(mut pipes) = shared.child.take()
        {
            let _ = pipes.proc.kill();
            let _ = pipes.proc.wait();
        }
    }
}

/// Whether a `java` runtime exists for the helper.
fn java_available() -> bool {
    Command::new("java")
        .arg("-version")
        .output()
        .is_ok_and(|out| out.status.success())
}

/// Write the helper next to itself atomically-ish (every writer emits
/// identical bytes) and launch it; a handshake query proves it runs.
fn spawn_helper() -> Option<Pipes> {
    if !java_available() {
        return None;
    }
    let path = std::env::temp_dir().join("noxis-jdk-hierarchy.java");
    let staging = path.with_extension(format!("java.{}", std::process::id()));
    std::fs::write(&staging, HELPER).ok()?;
    std::fs::rename(&staging, &path).ok()?;
    let mut proc = Command::new("java")
        .arg(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let rx = BufReader::new(proc.stdout.take()?);
    let tx = BufWriter::new(proc.stdin.take()?);
    let mut pipes = Pipes { rx, tx, proc };
    // Handshake: `Object` has no superclass; any answer at all means a wrong
    // runtime (or no source-launch support) - stay silent instead.
    match query(Some(&mut pipes), "java/lang/Object") {
        // The child must still be alive: EOF also reads as an empty line.
        Some(None) if pipes.proc.try_wait().is_ok_and(|exit| exit.is_none()) => Some(pipes),
        _ => None,
    }
}

/// One round trip: `None` - dead transport (child reaped);
/// `Some(answer)` - the reply, empty string for unknown superclasses.
fn query(pipes: Option<&mut Pipes>, name: &str) -> Option<Option<String>> {
    let pipes = pipes?;
    if writeln!(pipes.tx, "{name}").is_err() || pipes.tx.flush().is_err() {
        kill(pipes);
        return None;
    }
    let mut line = String::new();
    if pipes.rx.read_line(&mut line).is_err() {
        kill(pipes);
        return None;
    }
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Some(None);
    }
    Some(Some(trimmed.to_string()))
}

fn kill(pipes: &mut Pipes) {
    let _ = pipes.proc.kill();
    let _ = pipes.proc.wait();
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use noxis_classfile::Program;

    use super::*;

    #[test]
    fn jdk_superclasses_come_from_the_runtime() {
        if !java_available() {
            eprintln!("SKIP: no java");
            return;
        }
        let program = Program::default();
        let base = ClassHierarchy::build(&program);
        let jdk = JdkHierarchy::wrap(&base);
        // Real, stable JDK facts (unchanged for decades).
        assert_eq!(
            jdk.superclass("java/io/BufferedReader"),
            Some("java/io/Reader".to_string())
        );
        assert_eq!(
            jdk.superclass("java/lang/NoSuchMethodException"),
            Some("java/lang/ReflectiveOperationException".to_string())
        );
        assert_eq!(jdk.superclass("java/lang/Object"), None);
        // Outside the JDK: honest miss, like before.
        assert_eq!(jdk.superclass("demo/Nope"), None);
    }
}
