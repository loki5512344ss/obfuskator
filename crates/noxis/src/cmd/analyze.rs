//! Команда `analyze`: разбор jar без изменений.

use std::path::PathBuf;

use anyhow::{Context as _, Result};
use noxis_analysis::Analysis;
use noxis_core::Config;

/// Аргументы анализа.
#[derive(Debug, clap::Args)]
pub struct Args {
    /// Входной jar.
    pub input: PathBuf,
    /// TOML-конфиг с keep-паттернами (без него - пустой keep).
    #[arg(short, long)]
    pub config: Option<PathBuf>,
}

/// Показать классы, entrypoint'ы и keep-правила.
pub fn run(args: Args) -> Result<()> {
    let config = match &args.config {
        Some(path) => {
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("конфиг {}", path.display()))?;
            Config::from_toml(&text).map_err(|error| anyhow::anyhow!("{error}"))?
        }
        None => Config::default(),
    };
    let entries = noxis_jar::read_file(&args.input)
        .with_context(|| format!("чтение {}", args.input.display()))?;
    let program =
        noxis_jar::load(entries).map_err(|error| anyhow::anyhow!("разбор jar: {error}"))?;
    let analysis =
        Analysis::run(&program, &config).map_err(|error| anyhow::anyhow!("анализ: {error}"))?;

    println!("классов: {}", program.classes.len());
    println!("ресурсов: {}", program.resources.len());
    println!("locked классов: {}", analysis.locked_classes.len());
    println!(
        "entrypoints.main: {}",
        analysis.entrypoints.main_classes.len()
    );
    for name in &analysis.entrypoints.main_classes {
        println!("  main: {name}");
    }
    println!(
        "reflective_names: {}",
        analysis.entrypoints.reflective_names.len()
    );
    println!("keep-правил: {}", analysis.keep.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use super::*;

    #[test]
    fn analyze_missing_jar_fails() {
        let args = Args {
            input: PathBuf::from("definitely-not-here.jar"),
            config: None,
        };
        assert!(run(args).is_err());
    }
}
