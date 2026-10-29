//! Snapshots: a hidden git repository per synced project, kept in Canopy's
//! data folder (not in your project folder, which stays clean).
//!
//! Every sync commits what both sides agreed on to `main`, and points
//! `refs/canopy/synced` at it: that commit is the base for the next three-way
//! merge. The history of `main` is the snapshot list the app shows. It's a
//! plain bare repository, so `git log` works on it too.

use crate::files::Files;
use git2::{ObjectType, Oid, Repository, Signature, TreeBuilder};
use serde::Serialize;
use std::path::Path;

const MAIN: &str = "refs/heads/main";
const SYNCED: &str = "refs/canopy/synced";

/// One entry in the history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Snapshot {
    /// The commit id.
    pub id: String,
    /// Seconds since 1970, UTC.
    pub time: i64,
    /// What happened, e.g. "Synced: 2 down, 1 up".
    pub message: String,
}

/// How a file differs from the snapshot before.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChangeKind {
    /// New in this snapshot.
    Added,
    /// Changed contents.
    Modified,
    /// Gone in this snapshot.
    Deleted,
}

/// A file a snapshot changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Change {
    /// The project path.
    pub path: String,
    /// Added, modified or deleted.
    pub kind: ChangeKind,
}

/// A project's snapshot repository.
pub struct Store {
    repo: Repository,
}

impl Store {
    /// Opens the repository at `dir`, creating it the first time.
    pub fn open(dir: &Path) -> Result<Self, git2::Error> {
        let repo = match Repository::open_bare(dir) {
            Ok(r) => r,
            Err(_) => {
                std::fs::create_dir_all(dir).map_err(|e| git2::Error::from_str(&e.to_string()))?;
                let r = Repository::init_bare(dir)?;
                r.set_head(MAIN)?;
                r
            }
        };
        Ok(Self { repo })
    }

    /// The files as of the last sync (empty before the first one).
    pub fn base(&self) -> Result<Files, git2::Error> {
        match self.repo.refname_to_id(SYNCED) {
            Ok(id) => self.files_at(&id.to_string()),
            Err(_) => Ok(Files::new()),
        }
    }

    /// Whether this project has synced before.
    pub fn has_base(&self) -> bool {
        self.repo.refname_to_id(SYNCED).is_ok()
    }

    /// Records `files` as a new snapshot on `main`. With `synced`, it also
    /// becomes the base for the next merge. Returns the commit id, or the
    /// previous one when nothing changed.
    pub fn commit(
        &self,
        files: &Files,
        message: &str,
        synced: bool,
    ) -> Result<String, git2::Error> {
        let tree_id = self.write_tree(files)?;
        let parent = self
            .repo
            .refname_to_id(MAIN)
            .ok()
            .and_then(|id| self.repo.find_commit(id).ok());
        let id = match &parent {
            // an identical tree isn't worth a snapshot
            Some(p) if p.tree_id() == tree_id => p.id(),
            _ => {
                let tree = self.repo.find_tree(tree_id)?;
                // local only, never pushed anywhere
                let sig = Signature::now("Canopy", "canopy@localhost")?;
                let parents: Vec<&git2::Commit> = parent.iter().collect();
                self.repo
                    .commit(Some(MAIN), &sig, &sig, message, &tree, &parents)?
            }
        };
        if synced {
            self.repo.reference(SYNCED, id, true, "sync")?;
        }
        Ok(id.to_string())
    }

    // builds nested trees from "a/b/c.tex" paths
    fn write_tree(&self, files: &Files) -> Result<Oid, git2::Error> {
        #[derive(Default)]
        struct Dir {
            files: Vec<(String, Oid)>,
            dirs: std::collections::BTreeMap<String, Dir>,
        }
        let mut root = Dir::default();
        for (path, data) in files {
            let blob = self.repo.blob(data)?;
            let mut parts: Vec<&str> = path.split('/').collect();
            let name = parts.pop().unwrap_or_default().to_string();
            let mut dir = &mut root;
            for p in parts {
                dir = dir.dirs.entry(p.to_string()).or_default();
            }
            dir.files.push((name, blob));
        }
        fn build(repo: &Repository, dir: &Dir) -> Result<Oid, git2::Error> {
            let mut tb: TreeBuilder = repo.treebuilder(None)?;
            for (name, oid) in &dir.files {
                tb.insert(name, *oid, 0o100644)?;
            }
            for (name, sub) in &dir.dirs {
                let oid = build(repo, sub)?;
                tb.insert(name, oid, 0o040000)?;
            }
            tb.write()
        }
        build(&self.repo, &root)
    }

    /// Every file in a snapshot.
    pub fn files_at(&self, id: &str) -> Result<Files, git2::Error> {
        let commit = self.repo.find_commit(Oid::from_str(id)?)?;
        let tree = commit.tree()?;
        let mut out = Files::new();
        let mut err = None;
        tree.walk(git2::TreeWalkMode::PreOrder, |dir, entry| {
            if entry.kind() == Some(ObjectType::Blob) {
                let name = entry.name().unwrap_or_default();
                match entry.to_object(&self.repo).and_then(|o| o.peel_to_blob()) {
                    Ok(blob) => {
                        out.insert(format!("{dir}{name}"), blob.content().to_vec());
                    }
                    Err(e) => {
                        err = Some(e);
                        return git2::TreeWalkResult::Abort;
                    }
                }
            }
            git2::TreeWalkResult::Ok
        })?;
        match err {
            Some(e) => Err(e),
            None => Ok(out),
        }
    }

    /// The newest `limit` snapshots, newest first.
    pub fn history(&self, limit: usize) -> Result<Vec<Snapshot>, git2::Error> {
        let Ok(head) = self.repo.refname_to_id(MAIN) else {
            return Ok(Vec::new());
        };
        let mut walk = self.repo.revwalk()?;
        walk.push(head)?;
        walk.set_sorting(git2::Sort::TIME)?;
        walk.take(limit)
            .map(|id| {
                let c = self.repo.find_commit(id?)?;
                Ok(Snapshot {
                    id: c.id().to_string(),
                    time: c.time().seconds(),
                    message: c.message().unwrap_or_default().trim().to_string(),
                })
            })
            .collect()
    }

    /// What a snapshot changed compared with the one before it.
    pub fn changes(&self, id: &str) -> Result<Vec<Change>, git2::Error> {
        let commit = self.repo.find_commit(Oid::from_str(id)?)?;
        let new_tree = commit.tree()?;
        let old_tree = commit.parent(0).ok().map(|p| p.tree()).transpose()?;
        let diff = self
            .repo
            .diff_tree_to_tree(old_tree.as_ref(), Some(&new_tree), None)?;
        Ok(diff
            .deltas()
            .filter_map(|d| {
                let kind = match d.status() {
                    git2::Delta::Added => ChangeKind::Added,
                    git2::Delta::Deleted => ChangeKind::Deleted,
                    git2::Delta::Modified => ChangeKind::Modified,
                    _ => return None,
                };
                let file = if kind == ChangeKind::Deleted {
                    d.old_file()
                } else {
                    d.new_file()
                };
                let path = file.path()?.to_string_lossy().replace('\\', "/");
                Some(Change { path, kind })
            })
            .collect())
    }

    /// How one file changed in a snapshot, as a unified diff against the
    /// snapshot before it. Binary files get a one-line note instead.
    pub fn diff(&self, id: &str, path: &str) -> Result<String, git2::Error> {
        let commit = self.repo.find_commit(Oid::from_str(id)?)?;
        let before = match commit.parent(0) {
            Ok(p) => self.file_at(&p.id().to_string(), path)?,
            Err(_) => None,
        };
        let after = self.file_at(id, path)?;
        let text = |d: &Option<Vec<u8>>| -> Option<String> {
            match d {
                None => Some(String::new()),
                Some(b) if crate::files::is_text(b) => {
                    Some(String::from_utf8_lossy(b).into_owned())
                }
                Some(_) => None,
            }
        };
        Ok(match (text(&before), text(&after)) {
            (Some(old), Some(new)) => {
                let mut opts = diffy::DiffOptions::new();
                opts.set_context_len(3);
                let patch = opts.create_patch(&old, &new);
                format!("{patch}")
            }
            _ => format!(
                "Binary file {path}: {} -> {} bytes\n",
                before.map(|b| b.len()).unwrap_or(0),
                after.map(|b| b.len()).unwrap_or(0)
            ),
        })
    }

    /// One file as it was in a snapshot.
    pub fn file_at(&self, id: &str, path: &str) -> Result<Option<Vec<u8>>, git2::Error> {
        let commit = self.repo.find_commit(Oid::from_str(id)?)?;
        let tree = commit.tree()?;
        match tree.get_path(Path::new(path)) {
            Ok(entry) => Ok(Some(
                entry
                    .to_object(&self.repo)?
                    .peel_to_blob()?
                    .content()
                    .to_vec(),
            )),
            Err(e) if e.code() == git2::ErrorCode::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(entries: &[(&str, &str)]) -> Files {
        entries
            .iter()
            .map(|(k, v)| (k.to_string(), v.as_bytes().to_vec()))
            .collect()
    }

    #[test]
    fn commits_reads_back_and_tracks_the_base() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("h.git")).unwrap();
        assert!(!store.has_base());
        assert!(store.base().unwrap().is_empty());

        let v1 = files(&[
            ("main.tex", "one\n"),
            ("ch/a.tex", "a\n"),
            ("ch/deep/b.tex", "b\n"),
        ]);
        let c1 = store.commit(&v1, "Synced", true).unwrap();
        assert_eq!(store.base().unwrap(), v1);
        assert_eq!(store.files_at(&c1).unwrap(), v1);

        // a local-only snapshot doesn't move the base
        let v2 = files(&[("main.tex", "two\n"), ("ch/a.tex", "a\n")]);
        let c2 = store.commit(&v2, "Snapshot", false).unwrap();
        assert_eq!(store.base().unwrap(), v1);

        let mut changes = store.changes(&c2).unwrap();
        changes.sort_by(|a, b| a.path.cmp(&b.path));
        assert_eq!(
            changes,
            vec![
                Change {
                    path: "ch/deep/b.tex".into(),
                    kind: ChangeKind::Deleted
                },
                Change {
                    path: "main.tex".into(),
                    kind: ChangeKind::Modified
                },
            ]
        );
        assert_eq!(
            store.file_at(&c1, "ch/deep/b.tex").unwrap().unwrap(),
            b"b\n"
        );
        assert_eq!(store.file_at(&c2, "ch/deep/b.tex").unwrap(), None);
        let diff = store.diff(&c2, "main.tex").unwrap();
        assert!(diff.contains("-one") && diff.contains("+two"), "{diff}");
        assert!(store.diff(&c2, "ch/deep/b.tex").unwrap().contains("-b"));

        let hist = store.history(10).unwrap();
        assert_eq!(hist.len(), 2);
        assert_eq!(hist[0].id, c2);
        assert_eq!(hist[1].message, "Synced");
    }

    #[test]
    fn identical_trees_dont_make_new_snapshots() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let f = files(&[("a.tex", "x")]);
        let a = store.commit(&f, "one", true).unwrap();
        let b = store.commit(&f, "two", true).unwrap();
        assert_eq!(a, b);
        assert_eq!(store.history(10).unwrap().len(), 1);
    }

    #[test]
    fn reopens_an_existing_store() {
        let dir = tempfile::tempdir().unwrap();
        let f = files(&[("a.tex", "x")]);
        Store::open(dir.path())
            .unwrap()
            .commit(&f, "one", true)
            .unwrap();
        assert_eq!(Store::open(dir.path()).unwrap().base().unwrap(), f);
    }
}
