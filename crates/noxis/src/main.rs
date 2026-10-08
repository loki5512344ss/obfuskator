//! CLI обфускатора: `obfuscate`, `analyze`, `patterns`.

use anyhow::Result;
use clap::{Parser, Subcommand};
use noxis::cmd::{analyze, obfuscate, patterns};

/// Обфускатор JVM-байткода на Rust.
#[derive(Debug, Parser)]
#[command(name = "noxis", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Обфусцировать jar: конвейер, маппинги, JSON-отчёт.
    Obfuscate(obfuscate::Args),
    /// Разобрать jar: классы, entrypoint'ы, keep-правила.
    Analyze(analyze::Args),
    /// Красный список паттернов: показать или прогнать по jar'у.
    Patterns(patterns::Args),
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();
    match Cli::parse().command {
        Commands::Obfuscate(args) => obfuscate::run(args),
        Commands::Analyze(args) => analyze::run(args),
        Commands::Patterns(args) => patterns::run(args),
    }
}
