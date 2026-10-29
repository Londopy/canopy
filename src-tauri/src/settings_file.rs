// settings_file.rs
// settings export/import as a .londopy file (kind "settings"). accounts stay
// behind: a sign-in belongs to the computer it was made on.

use crate::config::AppConfig;
use crate::londopy;
use anyhow::{bail, Context, Result};
use std::path::Path;

pub const KIND: &str = "settings";
const ENTRY: &str = "settings.toml";
const MAX_BYTES: u64 = 1024 * 1024;

pub fn export(path: &Path, cfg: &AppConfig) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let manifest = londopy::Manifest::new(KIND, "Canopy settings", &now);
    let body = cfg.shareable().to_toml()?;
    londopy::write(path, &manifest, &[(ENTRY, body.as_bytes())])
}

// returns the imported settings with the current accounts kept
pub fn import(path: &Path, current: &AppConfig) -> Result<AppConfig> {
    let mut file = londopy::LondopyFile::open(path)?;
    if file.manifest.app != londopy::APP_ID {
        bail!("this file belongs to {}, not Canopy", file.manifest.app);
    }
    if file.manifest.kind != KIND {
        bail!("this is a Canopy {} file, not settings", file.manifest.kind);
    }
    let raw = file.read_entry(ENTRY, MAX_BYTES)?;
    let text = String::from_utf8(raw).context("settings.toml is not UTF-8")?;
    let mut cfg = AppConfig::from_toml(&text)?;
    cfg.accounts = current.accounts.clone();
    cfg.links = current.links.clone();
    Ok(cfg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Account, Theme};

    #[test]
    fn export_import_roundtrip_keeps_local_accounts() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mine.londopy");

        let mut exported = AppConfig::default();
        exported.general.theme = Theme::Dark;
        exported.accounts.push(Account {
            id: "theirs".into(),
            label: "Elsewhere".into(),
            server: Default::default(),
        });
        export(&path, &exported).unwrap();

        let mut here = AppConfig::default();
        here.accounts.push(Account {
            id: "mine".into(),
            label: "Here".into(),
            server: Default::default(),
        });
        let got = import(&path, &here).unwrap();
        assert_eq!(got.general.theme, Theme::Dark);
        assert_eq!(got.accounts, here.accounts);
    }

    #[test]
    fn refuses_other_kinds() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("theme.londopy");
        let m = londopy::Manifest::new("theme", "x", "2026-10-28T00:00:00Z");
        londopy::write(&path, &m, &[("theme.toml", b"")]).unwrap();
        let e = import(&path, &AppConfig::default()).unwrap_err();
        assert!(e.to_string().contains("not settings"), "{e}");
    }
}
