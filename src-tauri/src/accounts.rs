// accounts.rs
// adding, renaming and removing Overleaf accounts in the config. each account
// is one server plus its own browser profile (see web.rs), so two accounts on
// the same server stay signed in separately.

use crate::config::{Account, AppConfig};
use anyhow::{anyhow, bail, Result};
use canopy_overleaf::Server;

const MAX_LABEL: usize = 60;

// a short id that's unique within this config. it names the account's
// webview and profile folder, so it stays [0-9a-f]
fn new_id(cfg: &AppConfig) -> String {
    let mut seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
        ^ u64::from(std::process::id()) << 32;
    loop {
        let id = format!("{:012x}", seed & 0xffff_ffff_ffff);
        if !cfg.accounts.iter().any(|a| a.id == id) {
            return id;
        }
        seed = seed.wrapping_add(0x9e37_79b9);
    }
}

fn clean_label(label: &str) -> Result<String> {
    let label: String = label.split_whitespace().collect::<Vec<_>>().join(" ");
    if label.is_empty() {
        bail!("give the account a name");
    }
    Ok(label.chars().take(MAX_LABEL).collect())
}

// the name an account gets when the user leaves it blank
pub fn default_label(server: &Server) -> String {
    if server.is_overleaf_com() {
        "Overleaf".into()
    } else {
        server.host().to_string()
    }
}

pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 32 && id.bytes().all(|b| b.is_ascii_hexdigit())
}

pub fn add(cfg: &mut AppConfig, label: &str, server: &str) -> Result<Account> {
    let server = Server::parse(server)?;
    let label = if label.trim().is_empty() {
        default_label(&server)
    } else {
        clean_label(label)?
    };
    let account = Account {
        id: new_id(cfg),
        label,
        server,
    };
    cfg.accounts.push(account.clone());
    Ok(account)
}

pub fn rename(cfg: &mut AppConfig, id: &str, label: &str) -> Result<()> {
    let label = clean_label(label)?;
    let account = find_mut(cfg, id)?;
    account.label = label;
    Ok(())
}

pub fn remove(cfg: &mut AppConfig, id: &str) -> Result<Account> {
    let i = cfg
        .accounts
        .iter()
        .position(|a| a.id == id)
        .ok_or_else(|| anyhow!("no account {id}"))?;
    Ok(cfg.accounts.remove(i))
}

pub fn find<'a>(cfg: &'a AppConfig, id: &str) -> Result<&'a Account> {
    cfg.accounts
        .iter()
        .find(|a| a.id == id)
        .ok_or_else(|| anyhow!("no account {id}"))
}

fn find_mut<'a>(cfg: &'a mut AppConfig, id: &str) -> Result<&'a mut Account> {
    cfg.accounts
        .iter_mut()
        .find(|a| a.id == id)
        .ok_or_else(|| anyhow!("no account {id}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_with_default_labels() {
        let mut cfg = AppConfig::default();
        let a = add(&mut cfg, "", "overleaf.com").unwrap();
        assert_eq!(a.label, "Overleaf");
        assert!(a.server.is_overleaf_com());
        let b = add(&mut cfg, "  ", "https://tex.example.org:8443/project").unwrap();
        assert_eq!(b.label, "tex.example.org:8443");
        assert_ne!(a.id, b.id);
        assert!(valid_id(&a.id) && valid_id(&b.id));
        assert_eq!(cfg.accounts.len(), 2);
    }

    #[test]
    fn ids_stay_unique() {
        let mut cfg = AppConfig::default();
        for _ in 0..50 {
            add(&mut cfg, "x", "overleaf.com").unwrap();
        }
        let mut ids: Vec<_> = cfg.accounts.iter().map(|a| a.id.clone()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 50);
    }

    #[test]
    fn refuses_bad_servers_and_labels() {
        let mut cfg = AppConfig::default();
        assert!(add(&mut cfg, "x", "http://tex.example.org").is_err());
        assert!(add(&mut cfg, "x", "").is_err());
        assert!(cfg.accounts.is_empty());
        let a = add(&mut cfg, "Uni", "tex.example.org").unwrap();
        assert!(rename(&mut cfg, &a.id, "   ").is_err());
        assert_eq!(cfg.accounts[0].label, "Uni");
    }

    #[test]
    fn renames_and_removes() {
        let mut cfg = AppConfig::default();
        let a = add(&mut cfg, "Uni  \n server", "tex.example.org").unwrap();
        assert_eq!(a.label, "Uni server");
        rename(&mut cfg, &a.id, &"long ".repeat(40)).unwrap();
        assert_eq!(cfg.accounts[0].label.chars().count(), MAX_LABEL);
        assert!(rename(&mut cfg, "nope", "x").is_err());
        assert_eq!(remove(&mut cfg, &a.id).unwrap().id, a.id);
        assert!(remove(&mut cfg, &a.id).is_err());
    }

    #[test]
    fn id_check() {
        assert!(valid_id("00ff12ab"));
        assert!(!valid_id(""));
        assert!(!valid_id("../x"));
        assert!(!valid_id(&"a".repeat(33)));
    }
}
