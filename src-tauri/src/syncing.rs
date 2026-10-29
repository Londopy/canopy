// syncing.rs
// folder sync in the app: linking projects to folders, running syncs (one at
// a time per project), the snapshot history, a watcher per folder and a
// periodic pull. the engine itself lives in crates/sync; this file wires it
// to accounts, sessions and the UI.
//
// only in the full build: canopy lite has no sync.

use crate::config::{self, AppConfig, SyncLink};
use crate::{accounts, web, AppState};
use canopy_overleaf::{Client, ProjectSummary};
use canopy_sync::{Change, Conflict, Options, OverleafRemote, Report, Snapshot, Store, SyncState};
use notify_debouncer_full::notify::RecursiveMode;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};

// how often linked projects pull changes made on overleaf
const POLL_EVERY: Duration = Duration::from_secs(180);
// a sync writes into the folder; the watcher reports those writes a couple of
// seconds later, and they mustn't start another sync. how long to remember them
const ECHO_WINDOW: Duration = Duration::from_secs(30);

type Watcher = notify_debouncer_full::Debouncer<
    notify_debouncer_full::notify::RecommendedWatcher,
    notify_debouncer_full::RecommendedCache,
>;

/// Where each link stands, for the UI.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkStatus {
    id: String,
    state: SyncState,
    // seconds since 1970
    last_sync: Option<i64>,
    summary: Option<String>,
    error: Option<String>,
    conflicts: Vec<Conflict>,
    // set when a sync stopped short of deleting most of a side
    mass_delete: Option<(usize, usize)>,
    // files the server refused this time (retried next sync), and files that
    // can't sync at all, as "path (why)"
    failed: Vec<String>,
    unsyncable: Vec<String>,
    // where binary conflicts kept the folder's version
    kept_copies: Vec<String>,
}

// what the last sync wrote into a link's folder: path -> hash of the bytes
// (None for a delete), so the watcher can tell its echo from a real edit
struct Echo {
    at: Instant,
    paths: HashMap<String, Option<u64>>,
}

#[derive(Default)]
pub struct Syncer {
    status: Mutex<HashMap<String, LinkStatus>>,
    // one sync at a time per link
    running: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    // links with a folder change waiting for a sync; at most one queued run each
    dirty: Mutex<HashSet<String>>,
    echoes: Mutex<HashMap<String, Echo>>,
    watchers: Mutex<HashMap<String, Watcher>>,
}

fn hash(data: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    data.hash(&mut h);
    h.finish()
}

fn err(e: impl std::fmt::Display) -> String {
    format!("{e:#}")
}

fn history_dir(link_id: &str) -> PathBuf {
    config::data_dir()
        .join("sync")
        .join(link_id)
        .join("history.git")
}

fn find_link(app: &AppHandle, id: &str) -> Result<(SyncLink, config::Account), String> {
    let state = app.state::<AppState>();
    let cfg = state.config.read().unwrap_or_else(|e| e.into_inner());
    let link = cfg
        .links
        .iter()
        .find(|l| l.id == id)
        .cloned()
        .ok_or("that project isn't linked")?;
    let account = accounts::find(&cfg, &link.account_id).map_err(err)?.clone();
    Ok((link, account))
}

fn set_status(app: &AppHandle, id: &str, f: impl FnOnce(&mut LinkStatus)) {
    let syncer = &app.state::<AppState>().syncer;
    let snapshot = {
        let mut all = crate::lock(&syncer.status);
        let s = all.entry(id.to_string()).or_insert_with(|| LinkStatus {
            id: id.to_string(),
            ..Default::default()
        });
        f(s);
        s.clone()
    };
    let _ = app.emit("sync-status", snapshot);
}

async fn client_for(app: &AppHandle, account: &config::Account) -> Result<Client, String> {
    let cookie = web::session_cookie(app, account).await?;
    Client::new(account.server.clone(), &cookie).map_err(err)
}

// ---------------------------------------------------------------- commands

#[tauri::command]
pub async fn list_projects(
    app: AppHandle,
    account_id: String,
) -> Result<Vec<ProjectSummary>, String> {
    let account = {
        let state = app.state::<AppState>();
        let cfg = state.config.read().unwrap_or_else(|e| e.into_inner());
        accounts::find(&cfg, &account_id).map_err(err)?.clone()
    };
    let mut projects = client_for(&app, &account)
        .await?
        .projects()
        .await
        .map_err(err)?;
    projects.sort_by_key(|p| p.name.to_lowercase());
    Ok(projects)
}

// a folder name from a project name: what every OS can store, never empty
fn folder_name(project: &str) -> String {
    let cleaned: String = project
        .chars()
        .map(|c| {
            if c.is_control() || r#"<>:"/\|?*"#.contains(c) {
                '-'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim().trim_end_matches(['.', ' ']).to_string();
    let cleaned: String = cleaned.chars().take(80).collect();
    if cleaned.is_empty() || cleaned.chars().all(|c| c == '.') {
        "Overleaf project".into()
    } else {
        cleaned
    }
}

/// Links a project to a new folder named after it inside `parent`, then
/// starts the first sync. The folder has to be new (or empty): syncing an
/// existing folder full of files would upload all of them into the project.
#[tauri::command]
pub async fn link_project(
    app: AppHandle,
    account_id: String,
    project_id: String,
    project_name: String,
    parent: PathBuf,
) -> Result<AppConfig, String> {
    if !parent.is_absolute() || !parent.is_dir() {
        return Err("pick a folder to put the project in".into());
    }
    if project_id.is_empty() || !project_id.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err("that isn't an Overleaf project id".into());
    }
    let folder = parent.join(folder_name(&project_name));
    // two links on one folder would push one project's files into the other
    {
        let state = app.state::<AppState>();
        let cfg = state.config.read().unwrap_or_else(|e| e.into_inner());
        if let Some(l) = cfg
            .links
            .iter()
            .find(|l| config::overlaps(&l.folder, &folder))
        {
            return Err(format!(
                "{} overlaps the folder of {} ({}). Pick a place outside it.",
                folder.display(),
                l.project_name,
                l.folder.display()
            ));
        }
    }
    if folder.exists() {
        let empty = std::fs::read_dir(&folder)
            .map(|mut d| d.next().is_none())
            .unwrap_or(false);
        if !empty {
            return Err(format!(
                "{} already exists and has files in it. Pick another place, or empty it first.",
                folder.display()
            ));
        }
    }
    std::fs::create_dir_all(&folder).map_err(err)?;

    let state = app.state::<AppState>();
    let (cfg, id) = crate::change_config(&state, |c| {
        accounts::find(c, &account_id)?;
        if c.links.iter().any(|l| config::overlaps(&l.folder, &folder)) {
            anyhow::bail!("another synced project uses that folder");
        }
        if c.links
            .iter()
            .any(|l| l.project_id == project_id && l.account_id == account_id)
        {
            anyhow::bail!("{project_name} is already synced");
        }
        let id = accounts::new_id_for(c.links.iter().map(|l| l.id.as_str()));
        c.links.push(SyncLink {
            id: id.clone(),
            account_id: account_id.clone(),
            project_id: project_id.clone(),
            project_name: project_name.clone(),
            folder: folder.clone(),
            auto_sync: true,
        });
        Ok(id)
    })?;
    crate::dlog!("linked project {project_id} to {}", folder.display());
    refresh_watchers(&app);
    request_sync(&app, &id);
    Ok(cfg)
}

/// Stops syncing a project. The folder stays as it is; with
/// `delete_history`, the snapshots go too.
#[tauri::command]
pub async fn unlink_project(
    app: AppHandle,
    link_id: String,
    delete_history: bool,
) -> Result<AppConfig, String> {
    if !accounts::valid_id(&link_id) {
        return Err("bad link id".into());
    }
    let lock = run_lock(&app, &link_id);
    let _running = lock.lock().await;
    let state = app.state::<AppState>();
    let (cfg, _) = crate::change_config(&state, |c| {
        let before = c.links.len();
        c.links.retain(|l| l.id != link_id);
        if c.links.len() == before {
            anyhow::bail!("that project isn't linked");
        }
        Ok(())
    })?;
    crate::lock(&state.syncer.watchers).remove(&link_id);
    crate::lock(&state.syncer.status).remove(&link_id);
    if delete_history {
        let _ = std::fs::remove_dir_all(config::data_dir().join("sync").join(&link_id));
    }
    Ok(cfg)
}

#[tauri::command]
pub async fn set_link_auto(app: AppHandle, link_id: String, on: bool) -> Result<AppConfig, String> {
    let state = app.state::<AppState>();
    let (cfg, _) = crate::change_config(&state, |c| {
        let link = c
            .links
            .iter_mut()
            .find(|l| l.id == link_id)
            .ok_or_else(|| anyhow::anyhow!("that project isn't linked"))?;
        link.auto_sync = on;
        Ok(())
    })?;
    refresh_watchers(&app);
    Ok(cfg)
}

#[tauri::command]
pub async fn sync_now(
    app: AppHandle,
    link_id: String,
    allow_mass_delete: bool,
) -> Result<Report, String> {
    run_sync(&app, &link_id, Options { allow_mass_delete }).await
}

#[tauri::command]
pub fn sync_statuses(state: State<'_, AppState>) -> Vec<LinkStatus> {
    crate::lock(&state.syncer.status)
        .values()
        .cloned()
        .collect()
}

#[tauri::command]
pub async fn link_history(link_id: String) -> Result<Vec<Snapshot>, String> {
    if !accounts::valid_id(&link_id) {
        return Err("bad link id".into());
    }
    let dir = history_dir(&link_id);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    tauri::async_runtime::spawn_blocking(move || Store::open(&dir).and_then(|s| s.history(200)))
        .await
        .map_err(err)?
        .map_err(err)
}

#[tauri::command]
pub async fn snapshot_changes(link_id: String, snapshot_id: String) -> Result<Vec<Change>, String> {
    let dir = checked_history(&link_id)?;
    tauri::async_runtime::spawn_blocking(move || Store::open(&dir)?.changes(&snapshot_id))
        .await
        .map_err(err)?
        .map_err(err)
}

#[tauri::command]
pub async fn snapshot_diff(
    link_id: String,
    snapshot_id: String,
    path: String,
) -> Result<String, String> {
    let dir = checked_history(&link_id)?;
    tauri::async_runtime::spawn_blocking(move || Store::open(&dir)?.diff(&snapshot_id, &path))
        .await
        .map_err(err)?
        .map_err(err)
}

/// What restoring did: how many files changed, the snapshot of the folder
/// from just before (so the restore can be undone), and the sync after it.
#[derive(Serialize)]
pub struct RestoreReport {
    changed: usize,
    before: String,
    sync: Report,
}

/// Puts the folder back to how it was in a snapshot, then syncs, which
/// sends the restored files to Overleaf. The folder as it was (unsynced edits
/// included) is saved as a "Before restore" snapshot first.
#[tauri::command]
pub async fn restore_snapshot(
    app: AppHandle,
    link_id: String,
    snapshot_id: String,
) -> Result<RestoreReport, String> {
    let (link, _) = find_link(&app, &link_id)?;
    let dir = checked_history(&link_id)?;
    let restored = {
        let lock = run_lock(&app, &link_id);
        let _running = lock.lock().await;
        let folder = link.folder.clone();
        tauri::async_runtime::spawn_blocking(move || {
            canopy_sync::restore(&folder, &dir, &snapshot_id).map_err(err)
        })
        .await
        .map_err(err)??
    };
    crate::dlog!(
        "restored {} file(s) in {}",
        restored.changed,
        link.folder.display()
    );
    // restoring can delete files on purpose, so this one sync may
    let sync = run_sync(
        &app,
        &link_id,
        Options {
            allow_mass_delete: true,
        },
    )
    .await?;
    Ok(RestoreReport {
        changed: restored.changed,
        before: restored.before,
        sync,
    })
}
#[tauri::command]
pub fn open_link_folder(app: AppHandle, link_id: String) -> Result<(), String> {
    let (link, _) = find_link(&app, &link_id)?;
    tauri_plugin_opener::open_path(&link.folder, None::<&str>).map_err(err)
}

fn checked_history(link_id: &str) -> Result<PathBuf, String> {
    if !accounts::valid_id(link_id) {
        return Err("bad link id".into());
    }
    let dir = history_dir(link_id);
    if dir.exists() {
        Ok(dir)
    } else {
        Err("this project has no history yet".into())
    }
}

// ---------------------------------------------------------------- running syncs

fn run_lock(app: &AppHandle, id: &str) -> Arc<tokio::sync::Mutex<()>> {
    crate::lock(&app.state::<AppState>().syncer.running)
        .entry(id.to_string())
        .or_default()
        .clone()
}

async fn run_sync(app: &AppHandle, id: &str, opts: Options) -> Result<Report, String> {
    let lock = run_lock(app, id);
    let _running = lock.lock().await;
    // whatever changed in the folder up to now, this run picks up
    crate::lock(&app.state::<AppState>().syncer.dirty).remove(id);
    let (link, account) = find_link(app, id)?;
    set_status(app, id, |s| {
        s.state = SyncState::Syncing;
        s.error = None;
    });

    let result = async {
        let client = client_for(app, &account).await?;
        let remote = OverleafRemote::new(client, link.project_id.clone());
        canopy_sync::sync(&remote, &link.folder, &history_dir(id), opts)
            .await
            .map_err(|e| match e {
                canopy_sync::SyncError::MassDelete { remote, local } => {
                    set_status(app, id, |s| s.mass_delete = Some((remote, local)));
                    e.to_string()
                }
                other => other.to_string(),
            })
    }
    .await;

    let now = chrono::Utc::now().timestamp();
    match &result {
        Ok(report) => {
            crate::dlog!("synced {}: {}", link.project_name, report.summary());
            remember_echo(app, id, &link.folder, report);
            set_status(app, id, |s| {
                s.state = if !report.failed.is_empty() {
                    SyncState::Error
                } else if report.conflicts.is_empty() {
                    SyncState::Synced
                } else {
                    SyncState::Conflict
                };
                s.error = (!report.failed.is_empty()).then(|| {
                    format!(
                        "{} file(s) didn't reach Overleaf and will be tried again",
                        report.failed.len()
                    )
                });
                s.last_sync = Some(now);
                s.summary = Some(report.summary());
                s.conflicts = report.conflicts.clone();
                s.failed = report.failed.clone();
                s.unsyncable = report.unsyncable.clone();
                s.kept_copies = report.kept_copies.clone();
                s.mass_delete = None;
            });
        }
        Err(e) => {
            crate::dlog!("sync of {} failed: {e}", link.project_name);
            set_status(app, id, |s| {
                s.state = SyncState::Error;
                s.error = Some(e.clone());
            });
        }
    }
    result
}

// records what the sync just wrote, with the folders it touched
fn remember_echo(app: &AppHandle, id: &str, folder: &Path, report: &Report) {
    let mut paths = HashMap::new();
    let written = report.downloaded.iter().chain(&report.kept_copies);
    for p in written {
        let data = canopy_sync::resolve(folder, p)
            .ok()
            .and_then(|t| std::fs::read(t).ok());
        paths.insert(p.clone(), data.as_deref().map(hash));
    }
    for p in &report.deleted_local {
        paths.insert(p.clone(), None);
    }
    // the folders created or pruned on the way
    let dirs: Vec<String> = paths
        .keys()
        .flat_map(|p| {
            let parts: Vec<&str> = p.split('/').collect();
            (1..parts.len())
                .map(|n| parts[..n].join("/"))
                .collect::<Vec<_>>()
        })
        .collect();
    for d in dirs {
        paths.entry(d).or_insert(None);
    }
    crate::lock(&app.state::<AppState>().syncer.echoes).insert(
        id.to_string(),
        Echo {
            at: Instant::now(),
            paths,
        },
    );
}

// whether a watcher event for `rel` is only the echo of the last sync's write
fn is_echo(app: &AppHandle, id: &str, folder: &Path, rel: &str) -> bool {
    let expected = {
        let state = app.state::<AppState>();
        let echoes = crate::lock(&state.syncer.echoes);
        match echoes
            .get(id)
            .filter(|e| e.at.elapsed() < ECHO_WINDOW)
            .and_then(|e| e.paths.get(rel))
        {
            Some(h) => *h,
            None => return false,
        }
    };
    let expected = &expected;
    let target = folder.join(rel);
    if target.is_dir() {
        return true;
    }
    match (std::fs::read(&target).ok(), expected) {
        (Some(now), Some(h)) => hash(&now) == *h,
        (None, None) => true,
        _ => false,
    }
}

/// Queues a sync of a link: runs now, or right after the one in progress.
/// Any number of requests while one is queued add nothing, so a burst of
/// saves costs one download, not one each.
fn request_sync(app: &AppHandle, id: &str) {
    let queued = !crate::lock(&app.state::<AppState>().syncer.dirty).insert(id.to_string());
    if queued {
        return;
    }
    let (app, id) = (app.clone(), id.to_string());
    tauri::async_runtime::spawn(async move {
        let _ = run_sync(&app, &id, Options::default()).await;
    });
}
fn lite(app: &AppHandle) -> bool {
    app.state::<AppState>()
        .config
        .read()
        .map(|c| c.general.lite_mode)
        .unwrap_or(false)
}

/// Starts or stops folder watchers to match the config: one per linked
/// project with auto sync on, none in lite mode.
pub fn refresh_watchers(app: &AppHandle) {
    let wanted: Vec<SyncLink> = if lite(app) {
        Vec::new()
    } else {
        app.state::<AppState>()
            .config
            .read()
            .map(|c| c.links.iter().filter(|l| l.auto_sync).cloned().collect())
            .unwrap_or_default()
    };
    let state = app.state::<AppState>();
    let mut watchers = crate::lock(&state.syncer.watchers);
    watchers.retain(|id, _| wanted.iter().any(|l| &l.id == id));
    for link in wanted {
        if watchers.contains_key(&link.id) {
            continue;
        }
        match watch(app, &link) {
            Ok(w) => {
                watchers.insert(link.id.clone(), w);
            }
            Err(e) => crate::dlog!("can't watch {}: {e}", link.folder.display()),
        }
    }
}

fn watch(app: &AppHandle, link: &SyncLink) -> Result<Watcher, String> {
    let (app, id, root) = (app.clone(), link.id.clone(), link.folder.clone());
    let mut debouncer = notify_debouncer_full::new_debouncer(
        Duration::from_secs(2),
        None,
        move |result: notify_debouncer_full::DebounceEventResult| {
            let Ok(events) = result else { return };
            let changed = events
                .iter()
                .flat_map(|e| e.paths.iter())
                .filter_map(|p| relevant(&root, p))
                .any(|rel| !is_echo(&app, &id, &root, &rel));
            if changed {
                request_sync(&app, &id);
            }
        },
    )
    .map_err(err)?;
    debouncer
        .watch(&link.folder, RecursiveMode::Recursive)
        .map_err(err)?;
    Ok(debouncer)
}

// a change worth syncing: inside the folder and not something sync ignores.
// gives the project path
fn relevant(root: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    let rel = rel.to_string_lossy().replace('\\', "/");
    (!rel.is_empty() && !canopy_sync::ignore::ignored(&rel)).then_some(rel)
}

/// Pulls every auto-synced project every few minutes, starting shortly after
/// launch, for changes made on Overleaf.
pub fn start(app: &AppHandle) {
    refresh_watchers(app);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(10)).await;
        loop {
            if !lite(&app) {
                let ids: Vec<String> = app
                    .state::<AppState>()
                    .config
                    .read()
                    .map(|c| {
                        c.links
                            .iter()
                            .filter(|l| l.auto_sync)
                            .map(|l| l.id.clone())
                            .collect()
                    })
                    .unwrap_or_default();
                // one after another, to stay well under the server's rate limits
                for id in ids {
                    let _ = run_sync(&app, &id, Options::default()).await;
                }
            }
            tokio::time::sleep(POLL_EVERY).await;
        }
    });
}

/// Forgets every link of an account that's being removed, with its
/// snapshots (nothing could reach them without the account). Folders stay.
pub fn drop_account_links(app: &AppHandle, account_id: &str) {
    let state = app.state::<AppState>();
    let gone: Vec<String> = state
        .config
        .read()
        .map(|c| {
            c.links
                .iter()
                .filter(|l| l.account_id == account_id)
                .map(|l| l.id.clone())
                .collect()
        })
        .unwrap_or_default();
    let _ = crate::change_config(&state, |c| {
        c.links.retain(|l| l.account_id != account_id);
        Ok(())
    });
    for id in gone {
        crate::lock(&state.syncer.watchers).remove(&id);
        crate::lock(&state.syncer.status).remove(&id);
        if accounts::valid_id(&id) {
            let _ = std::fs::remove_dir_all(config::data_dir().join("sync").join(&id));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_names_are_storable() {
        assert_eq!(folder_name("Thesis: Draft 2"), "Thesis- Draft 2");
        assert_eq!(folder_name("a/b\\c?"), "a-b-c-");
        assert_eq!(folder_name("  ...  "), "Overleaf project");
        assert_eq!(folder_name("Notes."), "Notes");
        assert_eq!(folder_name(&"x".repeat(200)).len(), 80);
    }

    #[test]
    fn watcher_relevance() {
        let root = Path::new("/p");
        assert_eq!(
            relevant(root, Path::new("/p/main.tex")).as_deref(),
            Some("main.tex")
        );
        assert_eq!(
            relevant(root, Path::new("/p/ch/a.tex")).as_deref(),
            Some("ch/a.tex")
        );
        assert!(relevant(root, Path::new("/p/main.aux")).is_none());
        assert!(relevant(root, Path::new("/p/main.tex.canopy-tmp")).is_none());
        assert!(relevant(root, Path::new("/p/.git/index")).is_none());
        assert!(relevant(root, Path::new("/p/.GIT/index")).is_none());
        assert!(relevant(root, Path::new("/elsewhere/x.tex")).is_none());
        assert!(relevant(root, Path::new("/p")).is_none());
    }

    #[test]
    fn folder_overlap() {
        let base = std::env::temp_dir();
        let a = base.join("Thesis");
        assert!(config::overlaps(&a, &a));
        assert!(config::overlaps(&a, &a.join("chapters")));
        assert!(config::overlaps(&a.join("chapters"), &a));
        assert!(!config::overlaps(&a, &base.join("Thesis 2")));
        assert!(!config::overlaps(&a, &base.join("Other")));
        if cfg!(windows) {
            assert!(config::overlaps(&a, &base.join("THESIS")));
        }
    }
}
