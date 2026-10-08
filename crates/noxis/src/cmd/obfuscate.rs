//! Команда `obfuscate`: jar → конвейер → jar + маппинги + отчёт.

use std::path::PathBuf;

use anyhow::{Context as _, Result};
use noxis_core::{Config, Context, PassId};

/// Аргументы обфускации.
#[derive(Debug, clap::Args)]
pub struct Args {
    /// Входной jar.
    pub input: PathBuf,
    /// Выходной jar.
    #[arg(short, long)]
    pub output: PathBuf,
    /// TOML-конфиг (без него — профиль Standard).
    #[arg(short, long)]
    pub config: Option<PathBuf>,
    /// Мастер-сид (детерминированность по умолчанию — из конфига).
    #[arg(long)]
    pub seed: Option<u64>,
    /// Куда положить JSON-отчёт (по умолчанию рядом с выходом).
    #[arg(long)]
    pub report: Option<PathBuf>,
    /// Выключить пасс переименования.
    #[arg(long)]
    pub no_rename: bool,
    /// Выключить пасс ресурсов.
    #[arg(long)]
    pub no_resources: bool,
    /// Выключить пасс строк.
    #[arg(long)]
    pub no_strings: bool,
    /// Выключить пасс констант.
    #[arg(long)]
    pub no_constants: bool,
    /// Выключить пасс шума.
    #[arg(long)]
    pub no_noise: bool,
    /// Выключить пасс отладочной информации.
    #[arg(long)]
    pub no_debug_info: bool,
    /// Выключить пасс control-flow (ещё не реализован — честная ошибка).
    #[arg(long)]
    pub no_flow: bool,
}

/// Прогнать обфускацию.
pub fn run(args: Args) -> Result<()> {
    if args.no_flow {
        anyhow::bail!("пасс flow ещё не реализован (см. todo.md §3.4)");
    }
    let mut config = match &args.config {
        Some(path) => {
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("конфиг {}", path.display()))?;
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
        (args.no_noise, PassId::Noise),
        (args.no_debug_info, PassId::DebugInfo),
    ] {
        if flag && !config.disable.contains(&id) {
            config.disable.push(id);
        }
    }

    let entries = noxis_jar::read_file(&args.input)
        .with_context(|| format!("чтение {}", args.input.display()))?;
    let (entries, stripped) = noxis_jar::strip_signatures(entries);
    let mut program =
        noxis_jar::load(entries).map_err(|error| anyhow::anyhow!("разбор jar: {error}"))?;

    let mut ctx = Context::new(config);
    let report = noxis_passes::standard_pipeline()
        .run(&mut ctx, &mut program)
        .map_err(|error| anyhow::anyhow!("конвейер: {error}"))?;
    let entries =
        noxis_jar::save(&program).map_err(|error| anyhow::anyhow!("сборка jar: {error}"))?;
    noxis_jar::write_file(&args.output, &entries)
        .with_context(|| format!("запись {}", args.output.display()))?;

    let report_path = args
        .report
        .unwrap_or_else(|| args.output.with_extension("report.json"));
    std::fs::write(
        &report_path,
        serde_json::to_string_pretty(&SerializableReport::from(&report, &ctx))?,
    )
    .with_context(|| format!("отчёт {}", report_path.display()))?;
    std::fs::write(
        args.output.with_extension("mapping.txt"),
        ctx.mapping.to_text(),
    )?;

    println!(
        "noxis: {} → {} (классов: {}, подписей срезано: {}, отчёт: {})",
        args.input.display(),
        args.output.display(),
        report.counters.get("classes").copied().unwrap_or(0),
        stripped,
        report_path.display()
    );
    Ok(())
}

/// Отчёт в JSON: сид, пассы, счётчики, маппинги текстом.
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

    #[test]
    fn obfuscate_end_to_end() {
        let dir = std::env::temp_dir().join(format!("noxis-cli-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        let input = dir.join("in.jar");
        let output = dir.join("out.jar");
        sample_jar(&input);
        run(Args {
            input: input.clone(),
            output: output.clone(),
            config: None,
            seed: Some(5),
            report: None,
            no_rename: false,
            no_resources: false,
            no_strings: false,
            no_constants: false,
            no_noise: false,
            no_debug_info: false,
            no_flow: false,
        })
        .expect("obfuscate");
        assert!(output.exists(), "нет выходного jar");
        assert!(dir.join("out.report.json").exists(), "нет отчёта");
        assert!(dir.join("out.mapping.txt").exists(), "нет маппингов");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_flow_is_honest_error() {
        let args = Args {
            input: PathBuf::from("in.jar"),
            output: PathBuf::from("out.jar"),
            config: None,
            seed: None,
            report: None,
            no_rename: false,
            no_resources: false,
            no_strings: false,
            no_constants: false,
            no_noise: false,
            no_debug_info: false,
            no_flow: true,
        };
        assert!(run(args).is_err());
    }
}
