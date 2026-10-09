//! The `obfuscate` command: jar → pipeline → jar + mappings + report.

use std::path::PathBuf;

use anyhow::{Context as _, Result};
use noxis_core::{Config, Context, PassId};

/// Obfuscation arguments.
#[derive(Debug, clap::Args)]
pub struct Args {
    /// Input jar.
    pub input: PathBuf,
    /// Output jar.
    #[arg(short, long)]
    pub output: PathBuf,
    /// TOML config (without it — the Standard profile).
    #[arg(short, long)]
    pub config: Option<PathBuf>,
    /// Master seed (deterministic by default — taken from the config).
    #[arg(long)]
    pub seed: Option<u64>,
    /// Where to write the JSON report (next to the output by default).
    #[arg(long)]
    pub report: Option<PathBuf>,
    /// Disable the rename pass.
    #[arg(long)]
    pub no_rename: bool,
    /// Disable the resources pass.
    #[arg(long)]
    pub no_resources: bool,
    /// Disable the strings pass.
    #[arg(long)]
    pub no_strings: bool,
    /// Disable the constants pass.
    #[arg(long)]
    pub no_constants: bool,
    /// Disable the control-flow pass.
    #[arg(long)]
    pub no_flow: bool,
    /// Disable the noise pass.
    #[arg(long)]
    pub no_noise: bool,
    /// Disable the debug-info pass.
    #[arg(long)]
    pub no_debug_info: bool,
}

/// Run the obfuscation.
pub fn run(args: Args) -> Result<()> {
    let mut config = match &args.config {
        Some(path) => {
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("config {}", path.display()))?;
            Config::from_toml(&text).map_err(|error| anyhow::anyhow!("{error}"))?
        }
        None => Config::default(),
    };
    if let Some(seed) = args.seed {
        config.seed = Some(seed);
    }
    for (flag, id) in [
        (args.no_rename, PassId::Rename),
        (args.no_resources, PassId::Resources),
        (args.no_strings, PassId::Strings),
        (args.no_constants, PassId::Constants),
        (args.no_flow, PassId::Flow),
        (args.no_noise, PassId::Noise),
        (args.no_debug_info, PassId::DebugInfo),
    ] {
        if flag && !config.disable.contains(&id) {
            config.disable.push(id);
        }
    }

    let entries = noxis_jar::read_file(&args.input)
        .with_context(|| format!("reading {}", args.input.display()))?;
    let (entries, stripped) = noxis_jar::strip_signatures(entries);
    let mut program =
        noxis_jar::load(entries).map_err(|error| anyhow::anyhow!("parsing jar: {error}"))?;

    let mut ctx = Context::new(config);
    let report = noxis_passes::standard_pipeline()
        .run(&mut ctx, &mut program)
        .map_err(|error| anyhow::anyhow!("pipeline: {error}"))?;
    let entries =
        noxis_jar::save(&program).map_err(|error| anyhow::anyhow!("assembling jar: {error}"))?;
    noxis_jar::write_file(&args.output, &entries)
        .with_context(|| format!("writing {}", args.output.display()))?;

    let report_path = args
        .report
        .unwrap_or_else(|| args.output.with_extension("report.json"));
    std::fs::write(
        &report_path,
        serde_json::to_string_pretty(&SerializableReport::from(&report, &ctx))?,
    )
    .with_context(|| format!("report {}", report_path.display()))?;
    std::fs::write(
        args.output.with_extension("mapping.txt"),
        ctx.mapping.to_text(),
    )?;

    println!(
        "noxis: {} → {} (classes: {}, signatures stripped: {}, report: {})",
        args.input.display(),
        args.output.display(),
        report.counters.get("classes").copied().unwrap_or(0),
        stripped,
        report_path.display()
    );
    Ok(())
}

/// JSON report: seed, passes, counters, mappings as text.
#[derive(Debug, serde::Serialize)]
struct SerializableReport {
    seed: u64,
    passes: Vec<PassEntry>,
    counters: std::collections::BTreeMap<String, usize>,
    notes: Vec<String>,
    mapping: String,
}

#[derive(Debug, serde::Serialize)]
struct PassEntry {
    pass: String,
    transformed: usize,
    skipped: usize,
    notes: Vec<String>,
}

impl SerializableReport {
    fn from(report: &noxis_core::Report, ctx: &Context) -> Self {
        Self {
            seed: report.seed,
            passes: report
                .passes
                .iter()
                .map(|pass| PassEntry {
                    pass: pass.pass.to_string(),
                    transformed: pass.transformed,
                    skipped: pass.skipped,
                    notes: pass.notes.clone(),
                })
                .collect(),
            counters: report.counters.clone(),
            notes: report.notes.clone(),
            mapping: ctx.mapping.to_text(),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use super::*;

    fn sample_jar(path: &std::path::Path) {
        use noxis_classfile::{ClassBuilder, Code, Insn, Program, Version};

        let code = Code {
            max_stack: 0,
            max_locals: 0,
            insns: vec![Insn::Return(None)],
            exception_table: Vec::new(),
            frames: Vec::new(),
            attributes: Vec::new(),
        };
        let program = Program {
            classes: vec![
                ClassBuilder::new("demo/App", Some("java/lang/Object"))
                    .version(Version::JAVA_21)
                    .method_with_code(0x0009, "run", "()V", code)
                    .build(),
            ],
            resources: Vec::new(),
        };
        let entries = noxis_jar::save(&program).expect("save");
        noxis_jar::write_file(path, &entries).expect("write jar");
    }

    fn sample_args(input: PathBuf, output: PathBuf) -> Args {
        Args {
            input,
            output,
            config: None,
            seed: Some(5),
            report: None,
            no_rename: false,
            no_resources: false,
            no_strings: false,
            no_constants: false,
            no_flow: false,
            no_noise: false,
            no_debug_info: false,
        }
    }

    #[test]
    fn obfuscate_end_to_end() {
        let dir = std::env::temp_dir().join(format!("noxis-cli-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        let input = dir.join("in.jar");
        let output = dir.join("out.jar");
        sample_jar(&input);
        run(sample_args(input.clone(), output.clone())).expect("obfuscate");
        assert!(output.exists(), "missing output jar");
        assert!(dir.join("out.report.json").exists(), "missing report");
        assert!(dir.join("out.mapping.txt").exists(), "missing mappings");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_flow_disables_the_pass() {
        let dir = std::env::temp_dir().join(format!("noxis-cli-flow-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        let input = dir.join("in.jar");
        sample_jar(&input);
        let config_path = dir.join("paranoid.toml");
        std::fs::write(&config_path, "profile = \"paranoid\"\n").expect("write config");

        let run_with_flag = |name: &str, no_flow: bool| {
            let output = dir.join(format!("{name}.jar"));
            let report_path = dir.join(format!("{name}.report.json"));
            let mut args = sample_args(input.clone(), output);
            args.config = Some(config_path.clone());
            args.report = Some(report_path.clone());
            args.no_flow = no_flow;
            run(args).expect("obfuscate");
            std::fs::read_to_string(&report_path).expect("read report")
        };

        let with_flow = run_with_flag("with", false);
        assert!(
            with_flow.contains("\"flow\""),
            "flow pass missing under paranoid: {with_flow}"
        );
        let without_flow = run_with_flag("without", true);
        assert!(
            !without_flow.contains("\"flow\""),
            "flow pass ran despite --no-flow: {without_flow}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
