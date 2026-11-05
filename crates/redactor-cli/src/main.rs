//! `redactor`: redact pentest traffic before pasting it into an LLM.
//!
//! ```sh
//! pbpaste | redactor | pbcopy          # pipe
//! redactor --clipboard -e globex-q3    # rewrite the clipboard in place
//! redactor request.txt --report        # file, with a summary on stderr
//! redactor --remember -p               # redact the clipboard, remember values
//! redactor --restore -p                # put real values back in an LLM answer
//! redactor compare admin.xml user.xml  # compare two Burp site map exports
//! ```

mod clipboard;

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};
use redactor_core::sitemap::{self, ReportOptions};
use redactor_core::store::{GLOBAL_PROFILE, slugify};
use redactor_core::vault::VaultKey;
use redactor_core::{Category, Config, Redaction, Redactor, RestorePart, Store};

const EXAMPLE_CONFIG: &str = include_str!("../../../examples/config.toml");
const EXAMPLE_ENGAGEMENT: &str = include_str!("../../../examples/engagement.toml");

#[derive(Parser)]
#[command(version, about, long_about = None, args_conflicts_with_subcommands = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// File to redact. Reads stdin when omitted.
    input: Option<PathBuf>,

    /// Global config. Defaults to <config dir>/config.toml when it exists.
    #[arg(short, long, value_name = "PATH")]
    config: Option<PathBuf>,

    /// Engagement profile: a path, an id or a display name.
    #[arg(short, long, value_name = "NAME|PATH")]
    engagement: Option<String>,

    /// Read from and write back to the system clipboard (xclip on Linux).
    #[arg(short = 'p', long, conflicts_with = "input")]
    clipboard: bool,

    /// Print a summary of what was redacted to stderr.
    #[arg(short, long)]
    report: bool,

    /// Emit JSON with the output and the findings (original values excluded).
    #[arg(long)]
    json: bool,

    /// Remember redacted values in the profile's encrypted vault, so answers
    /// can be restored later with --restore.
    #[arg(long, conflicts_with = "restore")]
    remember: bool,

    /// Restore an LLM answer: put back the real values remembered with
    /// --remember. Ambiguous values are left as is and listed on stderr.
    #[arg(long, conflicts_with_all = ["json", "init"])]
    restore: bool,

    /// Create a starter config and engagement in the config dir, then exit.
    #[arg(long)]
    init: bool,
}

#[derive(Subcommand)]
enum Command {
    /// Compare two Burp site map exports (XML) of different roles and write a
    /// redacted report for access control review. Sends no requests.
    Compare(CompareArgs),
}

#[derive(Args)]
struct CompareArgs {
    /// Export of the role with more privileges.
    high: PathBuf,

    /// Export of the role with fewer privileges.
    low: PathBuf,

    /// Global config. Defaults to <config dir>/config.toml when it exists.
    #[arg(short, long, value_name = "PATH")]
    config: Option<PathBuf>,

    /// Engagement profile: a path, an id or a display name.
    #[arg(short, long, value_name = "NAME|PATH")]
    engagement: Option<String>,

    /// Names of the two roles in the report.
    #[arg(long, value_name = "HIGH,LOW", default_value = "admin,user")]
    labels: String,

    /// Write the report to this file instead of stdout.
    #[arg(long, value_name = "PATH")]
    report_out: Option<PathBuf>,

    /// Rows listed per section; the rest is only counted.
    #[arg(long, value_name = "N", default_value_t = 150)]
    max_rows: usize,

    /// Also add redacted responses of the endpoints whose label contains this
    /// text, for example "GET /api/users/{id}" (up to 3 per export).
    #[arg(long, value_name = "TEXT")]
    include_bodies: Option<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let store =
        Store::open_default().context("cannot locate the config dir; set REDACTOR_CONFIG_DIR")?;

    if let Some(Command::Compare(args)) = &cli.command {
        return compare(args, &store);
    }

    if cli.init {
        return init(&store);
    }

    let config = load_config(cli.config.as_deref(), cli.engagement.as_deref(), &store)?;
    let input = if cli.clipboard {
        clipboard::read()?
    } else {
        read_input(cli.input.as_deref())?
    };

    if cli.restore {
        return restore(&cli, &store, &input);
    }

    let redactor = Redactor::new(&config);
    let redaction = redactor.redact(&input);

    if cli.remember {
        let profile = profile(&cli, &store)?;
        let key = vault_key()?;
        let mut vault = store.load_vault(&profile, &key)?;
        vault.record_redaction(&redaction);
        store.save_vault(&profile, &vault, &key)?;
    }

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

fn restore(cli: &Cli, store: &Store, input: &str) -> Result<()> {
    let profile = profile(cli, store)?;
    let vault = store.load_vault(&profile, &vault_key()?)?;
    let restoration = vault.restore(input);
    let text = restoration.text();
    if cli.clipboard {
        clipboard::write(&text)?;
    } else {
        std::io::stdout().write_all(text.as_bytes())?;
    }
    for part in &restoration.parts {
        if let RestorePart::Ambiguous {
            replacement,
            candidates,
        } = part
        {
            eprintln!(
                "redactor: {replacement:?} is ambiguous: {}",
                candidates.join(" | ")
            );
        }
    }
    if cli.report || cli.clipboard {
        eprintln!(
            "redactor: {} values restored, {} ambiguous · profile={profile}",
            restoration.restored(),
            restoration.ambiguous()
        );
    }
    Ok(())
}

/// Vault profile: the engagement's id, or the global profile.
fn profile(cli: &Cli, store: &Store) -> Result<String> {
    let Some(engagement) = &cli.engagement else {
        return Ok(GLOBAL_PROFILE.to_string());
    };
    let path = Path::new(engagement);
    if path.exists() {
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(engagement);
        return Ok(slugify(stem));
    }
    Ok(store.find_engagement(engagement)?)
}

/// The vault key from the OS keychain. `REDACTOR_VAULT_KEY` (64 hex
/// characters) overrides it, for tests and automation only.
fn vault_key() -> Result<VaultKey> {
    if let Ok(hex) = std::env::var("REDACTOR_VAULT_KEY") {
        let bytes: Vec<u8> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(hex.get(i..i + 2).unwrap_or("zz"), 16))
            .collect::<Result<_, _>>()
            .context("REDACTOR_VAULT_KEY must be 64 hex characters")?;
        return bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("REDACTOR_VAULT_KEY must be 64 hex characters"));
    }
    Ok(redactor_core::keychain::vault_key()?)
}

fn compare(args: &CompareArgs, store: &Store) -> Result<()> {
    let config = load_config(args.config.as_deref(), args.engagement.as_deref(), store)?;
    let redactor = Redactor::new(&config);
    let high = read_site_map(&args.high)?;
    let low = read_site_map(&args.low)?;

    let (high_label, low_label) = args
        .labels
        .split_once(',')
        .context("--labels takes two names separated by a comma")?;
    let options = ReportOptions {
        high_label: high_label.to_string(),
        low_label: low_label.to_string(),
        max_rows: args.max_rows,
    };
    let mut samples = Vec::new();
    if let Some(filter) = &args.include_bodies {
        for (path, role) in [
            (&args.high, &options.high_label),
            (&args.low, &options.low_label),
        ] {
            let export = open_export(path)?;
            let found = sitemap::read_samples(export, role, filter, &redactor)
                .with_context(|| format!("reading {}", path.display()))?;
            samples.extend(found);
        }
    }

    let report = sitemap::report(&high, &low, &redactor, &options, &samples);
    match &args.report_out {
        Some(path) => {
            std::fs::write(path, &report).with_context(|| format!("writing {}", path.display()))?;
            eprintln!(
                "redactor: compared {} and {} endpoints, report in {}",
                high.endpoints.len(),
                low.endpoints.len(),
                path.display()
            );
        }
        None => std::io::stdout().write_all(report.as_bytes())?,
    }
    Ok(())
}

fn open_export(path: &Path) -> Result<BufReader<File>> {
    let file = File::open(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(BufReader::new(file))
}

fn read_site_map(path: &Path) -> Result<sitemap::SiteMap> {
    sitemap::read_map(open_export(path)?).with_context(|| format!("reading {}", path.display()))
}

fn load_config(config: Option<&Path>, engagement: Option<&str>, store: &Store) -> Result<Config> {
    let global = match config {
        Some(path) => Config::load(path)?,
        None => store.load_global()?,
    };
    let Some(engagement) = engagement else {
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
