//! On-disk layout shared by the CLI and the desktop app.
//!
//! ```text
//! <config dir>/
//! ├── config.toml              # global config
//! └── engagements/
//!     └── <id>.toml            # one profile per engagement
//! ```
//!
//! An engagement has a display name (`Globex Q3 — Web`, stored in the file's
//! `name` field) and an id derived from it (`globex-q3-web`) that is safe to
//! use as a file name.
//!
//! The config dir is `$REDACTOR_CONFIG_DIR`, `$XDG_CONFIG_HOME/redactor` or
//! `~/.config/redactor`, in that order. Files are written atomically and, on
//! Unix, readable only by the owner: they name clients and hosts.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::config::{Config, ConfigError};
use crate::dictionary::fold_char;

const GLOBAL_FILE: &str = "config.toml";
const ENGAGEMENTS_DIR: &str = "engagements";

/// Access to the config dir.
#[derive(Debug, Clone)]
pub struct Store {
    dir: PathBuf,
}

impl Store {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// The store at the default location, if one can be determined.
    pub fn open_default() -> Option<Self> {
        default_dir().map(Self::new)
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn global_path(&self) -> PathBuf {
        self.dir.join(GLOBAL_FILE)
    }

    pub fn engagement_path(&self, name: &str) -> Result<PathBuf, ConfigError> {
        validate_name(name)?;
        Ok(self.dir.join(ENGAGEMENTS_DIR).join(format!("{name}.toml")))
    }

    /// The global config, or the default one when the file does not exist.
    pub fn load_global(&self) -> Result<Config, ConfigError> {
        let path = self.global_path();
        if path.exists() {
            Config::load(path)
        } else {
            Ok(Config::default())
        }
    }

    pub fn save_global(&self, config: &Config) -> Result<(), ConfigError> {
        write_private(&self.global_path(), &config.to_toml()?)
    }

    /// Names of the saved engagements, sorted.
    pub fn engagements(&self) -> Result<Vec<String>, ConfigError> {
        let dir = self.dir.join(ENGAGEMENTS_DIR);
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let entries = std::fs::read_dir(&dir).map_err(|source| ConfigError::Io {
            path: dir.clone(),
            source,
        })?;
        let mut names: Vec<String> = entries
            .filter_map(|e| e.ok()?.path().file_name()?.to_str().map(str::to_string))
            .filter_map(|f| f.strip_suffix(".toml").map(str::to_string))
            .filter(|n| validate_name(n).is_ok())
            .collect();
        names.sort();
        Ok(names)
    }

    pub fn load_engagement(&self, name: &str) -> Result<Config, ConfigError> {
        Config::load(self.engagement_path(name)?)
    }

    pub fn save_engagement(&self, name: &str, config: &Config) -> Result<(), ConfigError> {
        write_private(&self.engagement_path(name)?, &config.to_toml()?)
    }

    pub fn delete_engagement(&self, name: &str) -> Result<(), ConfigError> {
        let path = self.engagement_path(name)?;
        std::fs::remove_file(&path).map_err(|source| ConfigError::Io { path, source })
    }

    /// Creates an engagement from a display name and returns its id. The id
    /// gets a numeric suffix if another engagement already uses it.
    pub fn create_engagement(&self, display_name: &str) -> Result<String, ConfigError> {
        let display_name = display_name.trim();
        let base = slugify(display_name);
        let mut id = base.clone();
        let mut n = 2;
        while self.engagement_path(&id)?.exists() {
            id = format!("{base}-{n}");
            n += 1;
        }
        let config = Config {
            name: Some(display_name.to_string()),
            ..Default::default()
        };
        self.save_engagement(&id, &config)?;
        Ok(id)
    }

    /// Resolves an engagement given its id or its display name
    /// (case-insensitive), as users type either on the command line.
    pub fn find_engagement(&self, query: &str) -> Result<String, ConfigError> {
        let ids = self.engagements()?;
        if ids.iter().any(|id| id == query) {
            return Ok(query.to_string());
        }
        for id in &ids {
            let name = self.load_engagement(id)?.name;
            if name.is_some_and(|n| n.trim().eq_ignore_ascii_case(query.trim())) {
                return Ok(id.clone());
            }
        }
        let slug = slugify(query);
        ids.into_iter()
            .find(|id| *id == slug)
            .ok_or_else(|| ConfigError::UnknownEngagement(query.to_string()))
    }

    /// The global config with the named engagement layered on top.
    pub fn resolve(&self, engagement: Option<&str>) -> Result<Config, ConfigError> {
        let global = self.load_global()?;
        match engagement {
            Some(name) => Ok(global.with_engagement(&self.load_engagement(name)?)),
            None => Ok(global),
        }
    }
}

/// `$REDACTOR_CONFIG_DIR`, `$XDG_CONFIG_HOME/redactor` or `~/.config/redactor`.
pub fn default_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("REDACTOR_CONFIG_DIR") {
        return Some(dir.into());
    }
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(xdg).join("redactor"));
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(PathBuf::from(home).join(".config").join("redactor"))
}

/// Turns a display name into an id: lowercase ASCII, digits and dashes.
///
/// ```
/// use redactor_core::store::slugify;
/// assert_eq!(slugify("Globex Q3 — Web App"), "globex-q3-web-app");
/// assert_eq!(slugify("Añil Pagos"), "anil-pagos");
/// ```
pub fn slugify(name: &str) -> String {
    let mut slug = String::with_capacity(name.len());
    for c in name.chars().map(fold_char) {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    let slug: String = slug.trim_end_matches('-').chars().take(64).collect();
    let slug = slug.trim_end_matches('-').to_string();
    if slug.is_empty() {
        "engagement".into()
    } else {
        slug
    }
}

/// Engagement ids become file names: keep them boring.
fn validate_name(name: &str) -> Result<(), ConfigError> {
    let valid = !name.is_empty()
        && name.len() <= 64
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if valid {
        Ok(())
    } else {
        Err(ConfigError::InvalidName(name.to_string()))
    }
}

/// Writes through a temporary file and a rename, so a crash never leaves a
/// half-written config behind.
fn write_private(path: &Path, contents: &str) -> Result<(), ConfigError> {
    let io = |source| ConfigError::Io {
        path: path.to_path_buf(),
        source,
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(io)?;
    }
    let tmp = path.with_extension("toml.tmp");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(&tmp).map_err(io)?;
    file.write_all(contents.as_bytes()).map_err(io)?;
    file.sync_all().map_err(io)?;
    std::fs::rename(&tmp, path).map_err(io)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store(name: &str) -> Store {
        let dir =
            std::env::temp_dir().join(format!("redactor-store-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        Store::new(dir)
    }

    #[test]
    fn round_trips_global_and_engagements() {
        let store = temp_store("roundtrip");
        assert_eq!(store.load_global().unwrap(), Config::default());

        let global = Config {
            client: vec!["Globex Bank".into()],
            ..Default::default()
        };
        store.save_global(&global).unwrap();
        let engagement = Config {
            hosts: vec!["srv-db01".into()],
            ..Default::default()
        };
        store.save_engagement("globex-q3", &engagement).unwrap();
        store
            .save_engagement("initech", &Config::default())
            .unwrap();

        assert_eq!(store.load_global().unwrap(), global);
        assert_eq!(store.engagements().unwrap(), ["globex-q3", "initech"]);
        let resolved = store.resolve(Some("globex-q3")).unwrap();
        assert_eq!(resolved.client, ["Globex Bank"]);
        assert_eq!(resolved.hosts, ["srv-db01"]);

        store.delete_engagement("initech").unwrap();
        assert_eq!(store.engagements().unwrap(), ["globex-q3"]);
        std::fs::remove_dir_all(store.dir()).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn files_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let store = temp_store("private");
        store.save_global(&Config::default()).unwrap();
        let mode = std::fs::metadata(store.global_path())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        std::fs::remove_dir_all(store.dir()).unwrap();
    }

    #[test]
    fn creates_engagements_from_display_names() {
        let store = temp_store("create");
        assert_eq!(
            store.create_engagement("Globex Q3 Web").unwrap(),
            "globex-q3-web"
        );
        assert_eq!(
            store.create_engagement("globex q3 web").unwrap(),
            "globex-q3-web-2"
        );
        assert_eq!(
            store
                .load_engagement("globex-q3-web")
                .unwrap()
                .name
                .as_deref(),
            Some("Globex Q3 Web")
        );
        assert_eq!(
            store.find_engagement("GLOBEX Q3 WEB").unwrap(),
            "globex-q3-web"
        );
        assert_eq!(
            store.find_engagement("globex-q3-web-2").unwrap(),
            "globex-q3-web-2"
        );
        assert!(matches!(
            store.find_engagement("initech"),
            Err(ConfigError::UnknownEngagement(_))
        ));
        std::fs::remove_dir_all(store.dir()).unwrap();
    }

    #[test]
    fn slugs_are_valid_ids() {
        for name in ["", "   ", "***", "../../etc", "Ω Omega", &"x".repeat(200)] {
            let slug = slugify(name);
            assert!(validate_name(&slug).is_ok(), "{name:?} -> {slug:?}");
        }
    }

    #[test]
    fn rejects_path_traversal() {
        let store = Store::new("/tmp/x");
        for bad in ["../evil", "a/b", "", ".hidden", "a b"] {
            assert!(store.engagement_path(bad).is_err(), "{bad}");
        }
    }
}
