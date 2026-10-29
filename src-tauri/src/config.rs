// config.rs
// settings live in config.toml in the data dir. loading is forgiving: a
// missing or unknown key, a value this version doesn't know (a theme from a
// newer release) or one bad account only resets that one thing, so a file
// from a newer version still opens and nothing else gets thrown away.

use anyhow::{Context, Result};
use canopy_overleaf::Server;
use canopy_texkit::EnginePref;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const APP_DIR: &str = "Canopy";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AppConfig {
    pub general: General,
    pub compile: Compile,
    pub updater: Updater,
    pub accounts: Vec<Account>,
    // projects synced to folders on this computer
    pub links: Vec<SyncLink>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct General {
    pub theme: Theme,
    // lite mode: web mode only, no file watcher, no language server
    pub lite_mode: bool,
    // closing the window hides it to the tray instead of quitting
    pub close_to_tray: bool,
    pub start_with_system: bool,
}

impl Default for General {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            lite_mode: false,
            close_to_tray: true,
            start_with_system: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Compile {
    pub engine: EnginePref,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Updater {
    pub auto_update: bool,
    // a version the user said "later" to; the banner stays quiet until a newer one
    pub skipped_version: String,
}

impl Default for Updater {
    fn default() -> Self {
        Self {
            auto_update: true,
            skipped_version: String::new(),
        }
    }
}

// one signed-in Overleaf server. the session cookie never goes in here; it
// will live in the OS keychain under the account id once sign-in lands
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub label: String,
    pub server: Server,
}

// an Overleaf project kept in step with a folder
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncLink {
    // hex, like account ids: it names the link's history folder
    pub id: String,
    pub account_id: String,
    pub project_id: String,
    pub project_name: String,
    pub folder: PathBuf,
    // sync on its own: when files in the folder change, and every few minutes
    #[serde(default = "yes")]
    pub auto_sync: bool,
}

fn yes() -> bool {
    true
}

// same folder, or one inside the other. case-insensitive on Windows and macOS,
// whose usual file systems are
pub fn overlaps(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| -> Vec<String> {
        let p = std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
        p.components()
            .map(|c| {
                let s = c.as_os_str().to_string_lossy().into_owned();
                if cfg!(any(windows, target_os = "macos")) {
                    s.to_lowercase()
                } else {
                    s
                }
            })
            .collect()
    };
    let (a, b) = (norm(a), norm(b));
    a.starts_with(&b) || b.starts_with(&a)
}

// %APPDATA%\Canopy, ~/Library/Application Support/Canopy, ~/.config/canopy,
// or the exe's folder when a `portable` marker sits next to it
pub fn data_dir() -> PathBuf {
    if let Some(dir) = portable_dir() {
        return dir;
    }
    platform_data_dir().unwrap_or_else(|| PathBuf::from("."))
}

pub fn is_portable() -> bool {
    portable_dir().is_some()
}

fn portable_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    dir.join("portable").is_file().then(|| dir.to_path_buf())
}

fn platform_data_dir() -> Option<PathBuf> {
    let env = |k: &str| {
        std::env::var_os(k)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    if cfg!(windows) {
        env("APPDATA").map(|d| d.join(APP_DIR))
    } else if cfg!(target_os = "macos") {
        env("HOME").map(|h| h.join("Library/Application Support").join(APP_DIR))
    } else {
        env("XDG_CONFIG_HOME")
            .or_else(|| env("HOME").map(|h| h.join(".config")))
            .map(|d| d.join("canopy"))
    }
}

pub fn config_path() -> PathBuf {
    data_dir().join("config.toml")
}

impl AppConfig {
    // a broken file shouldn't brick the app: log it, keep a copy, use defaults
    pub fn load_or_default(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(text) => match Self::from_toml(&text) {
                Ok(cfg) => cfg,
                Err(e) => {
                    crate::dlog!("config.toml unreadable, using defaults: {e:#}");
                    let _ = std::fs::copy(path, path.with_extension("toml.broken"));
                    Self::default()
                }
            },
            Err(_) => Self::default(),
        }
    }

    // only a TOML syntax error fails; bad values fall back one by one
    pub fn from_toml(text: &str) -> Result<Self> {
        let root: toml::Table = toml::from_str(text).context("Parsing config.toml")?;
        let accounts = match root.get("accounts") {
            Some(toml::Value::Array(items)) => items
                .iter()
                .filter_map(|v| match v.clone().try_into::<Account>() {
                    // the id names a webview and a profile folder, so it must
                    // be one canopy made: hex digits only
                    Ok(a) if !crate::accounts::valid_id(&a.id) => {
                        crate::dlog!("config.toml: skipping an account with id {:?}", a.id);
                        None
                    }
                    Ok(a) => Some(a),
                    Err(e) => {
                        crate::dlog!("config.toml: skipping an account: {e}");
                        None
                    }
                })
                .collect(),
            _ => Vec::new(),
        };
        let links = match root.get("links") {
            Some(toml::Value::Array(items)) => items
                .iter()
                .filter_map(|v| match v.clone().try_into::<SyncLink>() {
                    // ids end up in paths and URLs: only the ones canopy makes
                    Ok(l)
                        if crate::accounts::valid_id(&l.id)
                            && accounts.iter().any(|a| a.id == l.account_id)
                            && !l.project_id.is_empty()
                            && l.project_id.bytes().all(|b| b.is_ascii_alphanumeric())
                            && l.folder.is_absolute() =>
                    {
                        Some(l)
                    }
                    Ok(l) => {
                        crate::dlog!("config.toml: skipping the sync link {:?}", l.id);
                        None
                    }
                    Err(e) => {
                        crate::dlog!("config.toml: skipping a sync link: {e}");
                        None
                    }
                })
                .fold(Vec::<SyncLink>::new(), |mut kept, l| {
                    // two links on one folder would mix two projects' files
                    if kept.iter().any(|k| overlaps(&k.folder, &l.folder)) {
                        crate::dlog!("config.toml: skipping the overlapping sync link {:?}", l.id);
                    } else {
                        kept.push(l);
                    }
                    kept
                }),
            _ => Vec::new(),
        };
        Ok(Self {
            general: section(root.get("general")),
            compile: section(root.get("compile")),
            updater: section(root.get("updater")),
            accounts,
            links,
        })
    }

    pub fn to_toml(&self) -> Result<String> {
        toml::to_string_pretty(self).context("Writing config.toml")
    }

    // temp file + rename so a crash mid-write never leaves half a config
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("Creating {}", dir.display()))?;
        }
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, self.to_toml()?)
            .with_context(|| format!("Writing {}", tmp.display()))?;
        std::fs::rename(&tmp, path).with_context(|| format!("Saving {}", path.display()))?;
        Ok(())
    }

    // what a settings export carries: everything except the accounts and the
    // synced folders, which belong to this computer
    pub fn shareable(&self) -> Self {
        Self {
            accounts: Vec::new(),
            links: Vec::new(),
            ..self.clone()
        }
    }
}

// a section with every key it can't use dropped, so those keys take their
// defaults and the rest of the section survives
fn section<T: DeserializeOwned + Default>(value: Option<&toml::Value>) -> T {
    let Some(toml::Value::Table(table)) = value else {
        return T::default();
    };
    if let Ok(parsed) = toml::Value::Table(table.clone()).try_into() {
        return parsed;
    }
    let usable: toml::Table = table
        .iter()
        .filter(|(k, v)| {
            let mut one = toml::Table::new();
            one.insert((*k).clone(), (*v).clone());
            let ok = toml::Value::Table(one).try_into::<T>().is_ok();
            if !ok {
                crate::dlog!("config.toml: ignoring {k} = {v}");
            }
            ok
        })
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    toml::Value::Table(usable).try_into().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_roundtrips() {
        let cfg = AppConfig::default();
        let back = AppConfig::from_toml(&cfg.to_toml().unwrap()).unwrap();
        assert_eq!(cfg, back);
    }

    #[test]
    fn full_roundtrips() {
        let cfg = AppConfig {
            general: General {
                theme: Theme::Dark,
                lite_mode: true,
                close_to_tray: false,
                start_with_system: true,
            },
            compile: Compile {
                engine: EnginePref::Tectonic,
            },
            updater: Updater {
                auto_update: false,
                skipped_version: "0.2.0".into(),
            },
            accounts: vec![Account {
                id: "a1".into(),
                label: "Uni server".into(),
                server: Server::parse("tex.example.org").unwrap(),
            }],
            links: vec![SyncLink {
                id: "b2".into(),
                account_id: "a1".into(),
                project_id: "6ae2e6a197229a2a6d2f5002".into(),
                project_name: "Thesis".into(),
                folder: std::env::temp_dir().join("Thesis"),
                auto_sync: false,
            }],
        };
        let back = AppConfig::from_toml(&cfg.to_toml().unwrap()).unwrap();
        assert_eq!(cfg, back);
    }

    #[test]
    fn drops_links_that_dont_add_up() {
        let folder = std::env::temp_dir().join("x");
        let link = |id: &str, account: &str, project: &str, folder: &str| {
            format!(
                "[[links]]\nid = \"{id}\"\naccount_id = \"{account}\"\nproject_id = \"{project}\"\nproject_name = \"P\"\nfolder = {folder}\n"
            )
        };
        let abs = toml::Value::String(folder.display().to_string()).to_string();
        let text = format!(
            "[[accounts]]\nid = \"a1\"\nlabel = \"x\"\nserver = \"https://tex.example.org\"\n{}{}{}{}",
            link("b1", "a1", "6ae2", &abs),
            link("b2", "nobody", "6ae2", &abs),
            link("b3", "a1", "../x", &abs),
            link("b4", "a1", "6ae2", "\"relative/folder\""),
        );
        let cfg = AppConfig::from_toml(&text).unwrap();
        assert_eq!(
            cfg.links.iter().map(|l| l.id.as_str()).collect::<Vec<_>>(),
            vec!["b1"]
        );
        // and a missing auto_sync means on
        assert!(cfg.links[0].auto_sync);
    }

    #[test]
    fn missing_and_unknown_keys_load() {
        let cfg = AppConfig::from_toml(
            "[general]\ntheme = \"light\"\nfuture_key = 1\n[brand_new]\nx = 2\n",
        )
        .unwrap();
        assert_eq!(cfg.general.theme, Theme::Light);
        assert!(cfg.general.close_to_tray);
        assert!(cfg.updater.auto_update);
    }

    #[test]
    fn bad_values_only_reset_themselves() {
        let text = r#"
[general]
theme = "sepia"
lite_mode = true
close_to_tray = "yes please"

[compile]
engine = "xelatex"

[updater]
auto_update = false

[[accounts]]
id = "00aa"
label = "Uni"
server = "https://tex.example.org"

[[accounts]]
id = "00bb"
label = "Plain http"
server = "http://tex.example.org"

[[accounts]]
id = "C:/evil"
label = "Not an id canopy made"
server = "https://tex.example.org"
"#;
        let cfg = AppConfig::from_toml(text).unwrap();
        assert_eq!(cfg.general.theme, Theme::System);
        assert!(cfg.general.lite_mode);
        assert!(cfg.general.close_to_tray);
        assert_eq!(cfg.compile.engine, EnginePref::Auto);
        assert!(!cfg.updater.auto_update);
        assert_eq!(cfg.accounts.len(), 1);
        assert_eq!(cfg.accounts[0].id, "00aa");
    }

    #[test]
    fn shareable_drops_accounts() {
        let mut cfg = AppConfig::default();
        cfg.accounts.push(Account {
            id: "a".into(),
            label: "x".into(),
            server: Server::default(),
        });
        assert!(cfg.shareable().accounts.is_empty());
    }

    #[test]
    fn save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("config.toml");
        let mut cfg = AppConfig::default();
        cfg.general.lite_mode = true;
        cfg.save(&path).unwrap();
        assert_eq!(AppConfig::load_or_default(&path), cfg);
    }

    #[test]
    fn broken_file_falls_back_and_keeps_a_copy() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "general = [[[").unwrap();
        assert_eq!(AppConfig::load_or_default(&path), AppConfig::default());
        assert!(path.with_extension("toml.broken").is_file());
    }
}
