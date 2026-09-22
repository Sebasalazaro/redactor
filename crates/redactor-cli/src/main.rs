//! `redactor`: redact pentest traffic before pasting it into an LLM.
//!
//! ```sh
//! pbpaste | redactor | pbcopy          # pipe
//! redactor --clipboard -e globex-q3    # rewrite the clipboard in place (macOS)
//! redactor request.txt --report        # file, with a summary on stderr
//! ```

mod clipboard;

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::Parser;
use redactor_core::{Category, Config, Redaction, Redactor, Store};

const EXAMPLE_CONFIG: &str = include_str!("../../../examples/config.toml");
const EXAMPLE_ENGAGEMENT: &str = include_str!("../../../examples/engagement.toml");

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// File to redact. Reads stdin when omitted.
    input: Option<PathBuf>,

    /// Global config. Defaults to <config dir>/config.toml when it exists.
    #[arg(short, long, value_name = "PATH")]
    config: Option<PathBuf>,

    /// Engagement profile: a path, an id or a display name.
    #[arg(short, long, value_name = "NAME|PATH")]
    engagement: Option<String>,

    /// Read from and write back to the system clipboard (macOS).
    #[arg(short = 'p', long, conflicts_with = "input")]
    clipboard: bool,

    /// Print a summary of what was redacted to stderr.
    #[arg(short, long)]
    report: bool,

    /// Emit JSON with the output and the findings (original values excluded).
    #[arg(long)]
    json: bool,

    /// Create a starter config and engagement in the config dir, then exit.
    #[arg(long)]
    init: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let store =
        Store::open_default().context("cannot locate the config dir; set REDACTOR_CONFIG_DIR")?;

    if cli.init {
        return init(&store);
    }

    let config = load_config(&cli, &store)?;
    let redactor = Redactor::new(&config);

    let input = if cli.clipboard {
        clipboard::read()?
    } else {
        read_input(cli.input.as_deref())?
    };
    let redaction = redactor.redact(&input);

    if cli.clipboard {
        clipboard::write(&redaction.output)?;
    } else if cli.json {
        let json = serde_json::to_string_pretty(&redaction)?;
        writeln!(std::io::stdout(), "{json}")?;
    } else {
        std::io::stdout().write_all(redaction.output.as_bytes())?;
    }

    if cli.report || cli.clipboard {
        eprintln!("{}", summary(&redaction, config.name.as_deref()));
    }
    Ok(())
}

fn load_config(cli: &Cli, store: &Store) -> Result<Config> {
    let global = match &cli.config {
        Some(path) => Config::load(path)?,
        None => store.load_global()?,
    };
    let Some(engagement) = &cli.engagement else {
        return Ok(global);
    };
    let path = Path::new(engagement);
    let profile = if path.exists() {
        Config::load(path)?
    } else {
        let id = store.find_engagement(engagement)?;
        store
            .load_engagement(&id)
            .with_context(|| format!("loading engagement {engagement:?}"))?
    };
    Ok(global.with_engagement(&profile))
}

fn read_input(path: Option<&Path>) -> Result<String> {
    match path {
        Some(path) => {
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))
        }
        None => {
            let mut buf = String::new();
            std::io::stdin().read_to_string(&mut buf)?;
            Ok(buf)
        }
    }
}

fn init(store: &Store) -> Result<()> {
    let config = store.global_path();
    let engagement = store.engagement_path("example")?;
    if config.exists() {
        bail!("{} already exists", config.display());
    }
    std::fs::create_dir_all(engagement.parent().unwrap())?;
    // Written verbatim (not through Store::save_*) to keep the comments.
    std::fs::write(&config, EXAMPLE_CONFIG)?;
    std::fs::write(&engagement, EXAMPLE_ENGAGEMENT)?;
    eprintln!(
        "created {}\ncreated {}",
        config.display(),
        engagement.display()
    );
    Ok(())
}

fn summary(redaction: &Redaction, profile: Option<&str>) -> String {
    let mut counts: BTreeMap<Category, usize> = BTreeMap::new();
    for f in &redaction.findings {
        *counts.entry(f.category).or_default() += 1;
    }
    let details: Vec<String> = counts.iter().map(|(c, n)| format!("{c}={n}")).collect();
    format!(
        "redactor: {} values redacted ({}) · format={:?}{}",
        redaction.findings.len(),
        if details.is_empty() {
            "none".into()
        } else {
            details.join(", ")
        },
        redaction.format,
        profile
            .map(|p| format!(" · profile={p}"))
            .unwrap_or_default()
    )
}
