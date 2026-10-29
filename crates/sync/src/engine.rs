//! Carrying out a sync: fetch, plan, apply, record.
//!
//! Server changes go first; the folder is only touched once they've all gone
//! through, and the new base is only recorded at the very end. A sync cut off
//! halfway is therefore safe to run again: what was already uploaded simply
//! looks identical on both sides next time.

use crate::files::{self, Files, FilesError, Loaded, Skipped};
use crate::ignore::conflict_copy_name;
use crate::plan::{plan, Conflict, Step};
use crate::store::Store;
use serde::Serialize;
use std::future::Future;
use std::path::{Path, PathBuf};

/// The server side of a sync. The app implements it with the Overleaf
/// client; tests use an in-memory one.
pub trait Remote {
    /// Every file in the project, plus any it couldn't read.
    fn fetch(&self) -> impl Future<Output = Result<Loaded, String>> + Send;
    /// Creates or replaces one file.
    fn upload(&self, path: &str, data: Vec<u8>) -> impl Future<Output = Result<(), String>> + Send;
    /// Removes one file.
    fn delete(&self, path: &str) -> impl Future<Output = Result<(), String>> + Send;
}

/// Knobs for one sync.
#[derive(Debug, Clone, Copy, Default)]
pub struct Options {
    /// Go ahead even when the sync would delete most of a side's files
    /// (normally that stops it: it's usually a mistake, like the wrong folder).
    pub allow_mass_delete: bool,
}

/// Why a sync stopped.
#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    /// Talking to the server failed; nothing in the folder was changed.
    #[error("{0}")]
    Remote(String),
    /// Reading or writing the folder failed.
    #[error(transparent)]
    Files(#[from] FilesError),
    /// Writing the folder failed.
    #[error("couldn't write {path}: {source}")]
    Write {
        /// The project path.
        path: String,
        /// Why.
        source: std::io::Error,
    },
    /// The snapshot store failed.
    #[error("the snapshot history failed: {0}")]
    Store(#[from] git2::Error),
    /// The sync would delete most of one side; confirm with `allow_mass_delete`.
    #[error("this sync would delete {remote} file(s) on Overleaf and {local} in your folder")]
    MassDelete {
        /// Deletes on the server.
        remote: usize,
        /// Deletes in the folder.
        local: usize,
    },
}

/// What a sync did.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Report {
    /// Files written into the folder.
    pub downloaded: Vec<String>,
    /// Files sent to the server.
    pub uploaded: Vec<String>,
    /// Files removed from the folder.
    pub deleted_local: Vec<String>,
    /// Files removed from the server.
    pub deleted_remote: Vec<String>,
    /// Text both sides changed that merged cleanly.
    pub merged: Vec<String>,
    /// Files for the user to resolve.
    pub conflicts: Vec<Conflict>,
    /// Files changed in the folder during the sync; picked up next time.
    pub skipped: Vec<String>,
    /// Uploads and deletes the server refused, as "path: why"; retried next time.
    pub failed: Vec<String>,
    /// Files that can't sync at all (too large, a name some OS can't store),
    /// as "path (why)". Left alone on both sides.
    pub unsyncable: Vec<String>,
    /// Where binary conflicts kept the folder's version.
    pub kept_copies: Vec<String>,
    /// The snapshot recorded.
    pub snapshot: Option<String>,
}

impl Report {
    /// Whether the sync changed anything anywhere.
    pub fn is_quiet(&self) -> bool {
        self.downloaded.is_empty()
            && self.uploaded.is_empty()
            && self.deleted_local.is_empty()
            && self.deleted_remote.is_empty()
    }

    /// A one-line summary for the snapshot message and the UI.
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        let mut add = |n: usize, what: &str| {
            if n > 0 {
                parts.push(format!("{n} {what}"));
            }
        };
        add(self.downloaded.len(), "down");
        add(self.uploaded.len(), "up");
        add(
            self.deleted_local.len() + self.deleted_remote.len(),
            "deleted",
        );
        add(self.merged.len(), "merged");
        add(self.conflicts.len(), "conflicts");
        add(self.failed.len(), "failed");
        if parts.is_empty() {
            "Synced, no changes".into()
        } else {
            format!("Synced: {}", parts.join(", "))
        }
    }
}

// deleting a whole side, or more than half of it and more than a handful, is
// almost always a mistake: the wrong folder, a folder emptied by accident, a
// failed download
fn too_many(deletes: usize, of: usize) -> bool {
    deletes > 0 && (deletes >= of || (deletes > 5 && deletes * 2 > of))
}

// a path one side couldn't read counts as unchanged on that side, so a skip
// never looks like a delete
fn fill_skipped(side: &mut Files, skipped: &[Skipped], base: &Files) {
    for s in skipped {
        if let Some(b) = base.get(&s.path) {
            side.entry(s.path.clone()).or_insert_with(|| b.clone());
        }
    }
}

/// Syncs `folder` with `remote`, using the snapshot repository at `store_dir`.
pub async fn sync<R: Remote + Sync>(
    remote: &R,
    folder: &Path,
    store_dir: &Path,
    opts: Options,
) -> Result<Report, SyncError> {
    let fetched = remote.fetch().await.map_err(SyncError::Remote)?;

    // everything without an await, so the git handle never crosses one
    let (local, base, mut plan, unsyncable) = {
        let store = Store::open(store_dir)?;
        let base = store.base()?;
        let scanned = files::scan(folder)?;
        // the server's files, filtered by the same ignore rules as the folder's
        let mut remote_files: Files = fetched
            .files
            .into_iter()
            .filter(|(p, _)| !crate::ignore::ignored(p))
            .collect();
        let mut local = scanned.files;
        fill_skipped(&mut remote_files, &fetched.skipped, &base);
        fill_skipped(&mut local, &scanned.skipped, &base);

        let p = plan(&base, &local, &remote_files);
        let (del_remote, del_local) = p.deletes();
        if !opts.allow_mass_delete
            && (too_many(del_remote, base.len().max(remote_files.len()))
                || too_many(del_local, base.len().max(local.len())))
        {
            return Err(SyncError::MassDelete {
                remote: del_remote,
                local: del_local,
            });
        }
        let unsyncable: Vec<String> = fetched
            .skipped
            .iter()
            .chain(&scanned.skipped)
            .map(|s| s.to_string())
            .collect();
        (local, base, p, unsyncable)
    };

    let mut report = Report {
        merged: plan.merged.clone(),
        conflicts: plan.conflicts.clone(),
        unsyncable,
        ..Default::default()
    };

    // keeps a path's old base, so the next sync tries it again
    let keep_old_base = |plan_base: &mut Files, path: &str| match base.get(path) {
        Some(b) => {
            plan_base.insert(path.to_string(), b.clone());
        }
        None => {
            plan_base.remove(path);
        }
    };

    // 1. the server. one refused file doesn't stop the rest, or the folder's
    // half; it stays as it was and is retried next time
    for step in plan.steps.iter().filter(|s| s.is_remote()) {
        let (path, result) = match step {
            Step::Upload { path, data } => (path, remote.upload(path, data.clone()).await),
            Step::DeleteRemote { path } => (path, remote.delete(path).await),
            _ => continue,
        };
        match result {
            Ok(()) if matches!(step, Step::Upload { .. }) => report.uploaded.push(path.clone()),
            Ok(()) => report.deleted_remote.push(path.clone()),
            Err(e) => {
                report.failed.push(format!("{path}: {e}"));
                keep_old_base(&mut plan.base, path);
            }
        }
    }

    // 2. the folder
    for step in plan.steps.iter().filter(|s| !s.is_remote()) {
        let path = step.path().to_string();
        let target = crate::resolve(folder, &path).map_err(FilesError::from)?;
        // edited since we read it? leave it, and keep the old base for this
        // path so the next sync merges instead of overwriting either side
        if changed_since_scan(&target, local.get(&path)) {
            if !report.skipped.contains(&path) {
                report.skipped.push(path.clone());
            }
            keep_old_base(&mut plan.base, &path);
            continue;
        }
        // never write through a symlink or junction inside the folder
        if let Some(link) = linked_ancestor(folder, &target) {
            report.failed.push(format!(
                "{path}: {} is a link, so Canopy won't write through it",
                link.display()
            ));
            keep_old_base(&mut plan.base, &path);
            continue;
        }
        match step {
            Step::KeepLocalCopy { data, .. } => {
                // the first free name: earlier copies are ignored, so the
                // scan doesn't know about them
                let mut n = 1;
                let copy = loop {
                    let name = conflict_copy_name(&path, n);
                    let t = crate::resolve(folder, &name).map_err(FilesError::from)?;
                    if !t.exists() {
                        break (name, t);
                    }
                    n += 1;
                };
                // if this fails, the sync stops before the original is replaced
                write_atomic(&copy.1, data).map_err(|source| SyncError::Write {
                    path: copy.0.clone(),
                    source,
                })?;
                report.kept_copies.push(copy.0);
            }
            Step::WriteLocal { data, .. } => {
                write_atomic(&target, data).map_err(|source| SyncError::Write {
                    path: path.clone(),
                    source,
                })?;
                report.downloaded.push(path);
            }
            Step::DeleteLocal { .. } => {
                match std::fs::remove_file(&target) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(source) => return Err(SyncError::Write { path, source }),
                }
                prune_empty_dirs(folder, &target);
                report.deleted_local.push(path);
            }
            _ => {}
        }
    }

    // 3. the new base, as a snapshot
    let store = Store::open(store_dir)?;
    let id = store.commit(&plan.base, &report.summary(), true)?;
    if !report.is_quiet() {
        report.snapshot = Some(id);
    }
    Ok(report)
}

fn changed_since_scan(target: &Path, scanned: Option<&Vec<u8>>) -> bool {
    match (std::fs::read(target), scanned) {
        (Ok(now), Some(then)) => &now != then,
        (Ok(_), None) => true,
        (Err(_), Some(_)) => true,
        (Err(_), None) => false,
    }
}

// the first existing folder between `root` (exclusive) and `target` that is
// a symlink or junction, which a write would follow out of the folder
fn linked_ancestor(root: &Path, target: &Path) -> Option<PathBuf> {
    let mut dir = target.parent();
    while let Some(d) = dir {
        if d == root || !d.starts_with(root) {
            return None;
        }
        // std reports Windows junctions as symlinks too
        if std::fs::symlink_metadata(d).is_ok_and(|m| m.file_type().is_symlink()) {
            return Some(d.to_path_buf());
        }
        dir = d.parent();
    }
    None
}

// temp file and rename, so a crash never leaves half a file
fn write_atomic(target: &Path, data: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = target.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut tmp = target.as_os_str().to_owned();
    tmp.push(".canopy-tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, target).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

// removes folders a delete left empty, up to (not including) the root
fn prune_empty_dirs(root: &Path, file: &Path) {
    let mut dir = file.parent();
    while let Some(d) = dir {
        if d == root || !d.starts_with(root) || std::fs::remove_dir(d).is_err() {
            break;
        }
        dir = d.parent();
    }
}

/// What a restore did.
#[derive(Debug, Clone, Serialize)]
pub struct Restored {
    /// Files written or deleted in the folder.
    pub changed: usize,
    /// The snapshot of the folder as it was just before, so the restore can
    /// itself be undone.
    pub before: String,
}

/// Puts the folder back to snapshot `id`. First records the folder as it is
/// (unsynced edits, new files, unresolved conflicts included) as a
/// snapshot of its own; the next sync then uploads the restored state.
pub fn restore(folder: &Path, store_dir: &Path, id: &str) -> Result<Restored, SyncError> {
    let store = Store::open(store_dir)?;
    let current = files::scan(folder)?.files;
    let before = store.commit(&current, "Before restore", false)?;
    let files = store.files_at(id)?;
    let mut changed = 0;
    for (path, data) in &files {
        if current.get(path).is_some_and(|c| files::same(c, data)) {
            continue;
        }
        let target = crate::resolve(folder, path).map_err(FilesError::from)?;
        if linked_ancestor(folder, &target).is_some() {
            continue;
        }
        write_atomic(&target, data).map_err(|source| SyncError::Write {
            path: path.clone(),
            source,
        })?;
        changed += 1;
    }
    for path in current.keys().filter(|p| !files.contains_key(*p)) {
        let target = crate::resolve(folder, path).map_err(FilesError::from)?;
        std::fs::remove_file(&target).map_err(|source| SyncError::Write {
            path: path.clone(),
            source,
        })?;
        prune_empty_dirs(folder, &target);
        changed += 1;
    }
    Ok(Restored { changed, before })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Fake {
        files: Mutex<Files>,
        fail_uploads: bool,
    }

    impl Remote for Fake {
        async fn fetch(&self) -> Result<Loaded, String> {
            Ok(Loaded {
                files: self.files.lock().unwrap().clone(),
                skipped: Vec::new(),
            })
        }
        async fn upload(&self, path: &str, data: Vec<u8>) -> Result<(), String> {
            if self.fail_uploads || path.starts_with("bad") {
                return Err("refused".into());
            }
            self.files.lock().unwrap().insert(path.into(), data);
            Ok(())
        }
        async fn delete(&self, path: &str) -> Result<(), String> {
            self.files.lock().unwrap().remove(path);
            Ok(())
        }
    }

    fn fake(entries: &[(&str, &str)]) -> Fake {
        Fake {
            files: Mutex::new(
                entries
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.as_bytes().to_vec()))
                    .collect(),
            ),
            fail_uploads: false,
        }
    }

    fn block<F: Future>(f: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(f)
    }

    struct Setup {
        _dir: tempfile::TempDir,
        folder: PathBuf,
        store: PathBuf,
    }

    fn setup() -> Setup {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("project");
        std::fs::create_dir(&folder).unwrap();
        let store = dir.path().join("history.git");
        Setup {
            _dir: dir,
            folder,
            store,
        }
    }

    #[test]
    fn round_trip() {
        let s = setup();
        let remote = fake(&[("main.tex", "hello\n"), ("ch/a.tex", "a\n")]);

        // first sync pulls everything
        let r = block(sync(&remote, &s.folder, &s.store, Options::default())).unwrap();
        assert_eq!(r.downloaded.len(), 2);
        assert_eq!(
            std::fs::read_to_string(s.folder.join("ch").join("a.tex")).unwrap(),
            "a\n"
        );
        assert!(r.snapshot.is_some());

        // edit locally -> uploads
        std::fs::write(s.folder.join("main.tex"), "hello\r\nworld\r\n").unwrap();
        let r = block(sync(&remote, &s.folder, &s.store, Options::default())).unwrap();
        assert_eq!(r.uploaded, vec!["main.tex"]);
        assert_eq!(remote.files.lock().unwrap()["main.tex"], b"hello\nworld\n");

        // deleted on the server -> deleted here, and the empty folder goes too
        remote.files.lock().unwrap().remove("ch/a.tex");
        let r = block(sync(&remote, &s.folder, &s.store, Options::default())).unwrap();
        assert_eq!(r.deleted_local, vec!["ch/a.tex"]);
        assert!(!s.folder.join("ch").exists());

        // nothing to do
        let r = block(sync(&remote, &s.folder, &s.store, Options::default())).unwrap();
        assert!(r.is_quiet());
        assert_eq!(r.summary(), "Synced, no changes");
        assert_eq!(Store::open(&s.store).unwrap().history(10).unwrap().len(), 3);
    }

    #[test]
    fn a_refused_upload_is_retried_and_doesnt_block_the_rest() {
        let s = setup();
        let mut remote = fake(&[("main.tex", "v1\n"), ("other.tex", "o1\n")]);
        block(sync(&remote, &s.folder, &s.store, Options::default())).unwrap();

        // a local edit the server refuses, and a collaborator's edit to pull
        std::fs::write(s.folder.join("main.tex"), "v2\n").unwrap();
        remote.fail_uploads = true;
        remote
            .files
            .lock()
            .unwrap()
            .insert("other.tex".into(), b"o2\n".to_vec());
        let r = block(sync(&remote, &s.folder, &s.store, Options::default())).unwrap();
        assert_eq!(r.failed.len(), 1, "{r:?}");
        assert_eq!(r.downloaded, vec!["other.tex"]);
        let base = Store::open(&s.store).unwrap().base().unwrap();
        assert_eq!(base["main.tex"], b"v1\n");
        assert_eq!(base["other.tex"], b"o2\n");

        remote.fail_uploads = false;
        let r = block(sync(&remote, &s.folder, &s.store, Options::default())).unwrap();
        assert_eq!(r.uploaded, vec!["main.tex"]);
        assert!(r.failed.is_empty());
    }

    #[test]
    fn small_projects_are_guarded_too() {
        let s = setup();
        let remote = fake(&[("a.tex", "a\n"), ("b.tex", "b\n")]);
        block(sync(&remote, &s.folder, &s.store, Options::default())).unwrap();
        std::fs::remove_file(s.folder.join("a.tex")).unwrap();
        std::fs::remove_file(s.folder.join("b.tex")).unwrap();
        let err = block(sync(&remote, &s.folder, &s.store, Options::default())).unwrap_err();
        assert!(matches!(err, SyncError::MassDelete { remote: 2, .. }));
        assert_eq!(remote.files.lock().unwrap().len(), 2);
        // deleting some, not all, of a small project just goes ahead
        assert!(!too_many(1, 3));
        assert!(too_many(1, 1));
    }

    #[test]
    fn a_second_binary_conflict_keeps_both_copies() {
        let s = setup();
        let remote = Fake {
            files: Mutex::new([("fig.png".to_string(), vec![0u8, 1])].into()),
            fail_uploads: false,
        };
        block(sync(&remote, &s.folder, &s.store, Options::default())).unwrap();
        for round in 2u8..4 {
            std::fs::write(s.folder.join("fig.png"), [0u8, 10 + round]).unwrap();
            remote
                .files
                .lock()
                .unwrap()
                .insert("fig.png".into(), vec![0u8, 20 + round]);
            let r = block(sync(&remote, &s.folder, &s.store, Options::default())).unwrap();
            assert_eq!(r.kept_copies.len(), 1, "{r:?}");
        }
        assert_eq!(
            std::fs::read(s.folder.join("fig.local-conflict.png")).unwrap(),
            [0, 12]
        );
        assert_eq!(
            std::fs::read(s.folder.join("fig.local-conflict-2.png")).unwrap(),
            [0, 13]
        );
        assert_eq!(std::fs::read(s.folder.join("fig.png")).unwrap(), [0, 23]);
    }

    #[test]
    fn an_unsyncable_file_is_not_a_delete() {
        let s = setup();
        let remote = fake(&[("a.tex", "a\n")]);
        block(sync(&remote, &s.folder, &s.store, Options::default())).unwrap();
        // the file grows past the limit: skipped, and the server keeps its copy
        let big = vec![b'x'; (files::MAX_FILE + 1) as usize];
        std::fs::write(s.folder.join("a.tex"), &big).unwrap();
        let r = block(sync(&remote, &s.folder, &s.store, Options::default())).unwrap();
        assert!(
            r.deleted_remote.is_empty() && r.uploaded.is_empty(),
            "{r:?}"
        );
        assert_eq!(r.unsyncable.len(), 1);
        assert_eq!(remote.files.lock().unwrap()["a.tex"], b"a\n");
    }
    #[test]
    fn refuses_to_wipe_a_side() {
        let s = setup();
        let many: Vec<(String, String)> = (0..10)
            .map(|i| (format!("f{i}.tex"), "x\n".into()))
            .collect();
        let entries: Vec<(&str, &str)> =
            many.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
        let remote = fake(&entries);
        block(sync(&remote, &s.folder, &s.store, Options::default())).unwrap();

        // the folder got emptied: don't delete all of overleaf
        for (p, _) in &many {
            std::fs::remove_file(s.folder.join(p)).unwrap();
        }
        let err = block(sync(&remote, &s.folder, &s.store, Options::default())).unwrap_err();
        assert!(matches!(
            err,
            SyncError::MassDelete {
                remote: 10,
                local: 0
            }
        ));
        assert_eq!(remote.files.lock().unwrap().len(), 10);

        let r = block(sync(
            &remote,
            &s.folder,
            &s.store,
            Options {
                allow_mass_delete: true,
            },
        ))
        .unwrap();
        assert_eq!(r.deleted_remote.len(), 10);
    }

    #[test]
    fn conflicts_reach_the_folder_but_not_the_server() {
        let s = setup();
        let remote = fake(&[("a.tex", "line\n")]);
        block(sync(&remote, &s.folder, &s.store, Options::default())).unwrap();

        std::fs::write(s.folder.join("a.tex"), "mine\n").unwrap();
        remote
            .files
            .lock()
            .unwrap()
            .insert("a.tex".into(), b"theirs\n".to_vec());
        let r = block(sync(&remote, &s.folder, &s.store, Options::default())).unwrap();
        assert_eq!(r.conflicts.len(), 1);
        assert!(std::fs::read_to_string(s.folder.join("a.tex"))
            .unwrap()
            .contains("<<<<<<< your folder"));
        assert_eq!(remote.files.lock().unwrap()["a.tex"], b"theirs\n");

        // resolve, sync: the resolution uploads
        std::fs::write(s.folder.join("a.tex"), "agreed\n").unwrap();
        let r = block(sync(&remote, &s.folder, &s.store, Options::default())).unwrap();
        assert_eq!(r.uploaded, vec!["a.tex"]);
        assert_eq!(remote.files.lock().unwrap()["a.tex"], b"agreed\n");
    }

    #[test]
    fn restores_a_snapshot_into_the_folder() {
        let s = setup();
        let remote = fake(&[("a.tex", "one\n"), ("b.tex", "b\n")]);
        let r = block(sync(&remote, &s.folder, &s.store, Options::default())).unwrap();
        let first = r.snapshot.unwrap();

        std::fs::write(s.folder.join("a.tex"), "two\n").unwrap();
        std::fs::write(s.folder.join("c.tex"), "new\n").unwrap();
        let done = restore(&s.folder, &s.store, &first).unwrap();
        assert_eq!(done.changed, 2);
        assert_eq!(
            std::fs::read_to_string(s.folder.join("a.tex")).unwrap(),
            "one\n"
        );
        assert!(!s.folder.join("c.tex").exists());
        // the unsynced edits are in the "Before restore" snapshot
        let before = Store::open(&s.store)
            .unwrap()
            .files_at(&done.before)
            .unwrap();
        assert_eq!(before["a.tex"], b"two\n");
        assert_eq!(before["c.tex"], b"new\n");
    }
}
