//! Команда `patterns`: красный список и его прогон по jar'у.

use std::path::PathBuf;

use anyhow::{Context as _, Result};

/// Аргументы просмотра/проверки паттернов.
#[derive(Debug, clap::Args)]
pub struct Args {
    /// Показать только список (без проверки).
    #[arg(long)]
    pub list: bool,
    /// Jar для проверки на секреты открытым текстом.
    #[arg(short, long)]
    pub input: Option<PathBuf>,
}

/// Показать список или прогнать его по jar'у.
pub fn run(args: Args) -> Result<()> {
    if args.list || args.input.is_none() {
        for pattern in noxis_verify::RED_PATTERNS {
            println!("{pattern}");
        }
    }
    let Some(input) = args.input else {
        return Ok(());
    };
    let entries =
        noxis_jar::read_file(&input).with_context(|| format!("чтение {}", input.display()))?;
    let program =
        noxis_jar::load(entries).map_err(|error| anyhow::anyhow!("разбор jar: {error}"))?;
    let hits = noxis_verify::scan_secrets(&program);
    for hit in &hits {
        println!("{}: [{}] {:.48}", hit.location, hit.pattern, hit.sample);
    }
    println!("совпадений: {}", hits.len());
    Ok(())
}
