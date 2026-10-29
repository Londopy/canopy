//! A project's files as one map, from a server zip or a folder on disk.

use crate::ignore::ignored;
use crate::paths::{project_path, PathError};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

/// Every file in a project: relative `/`-separated path to contents.
pub type Files = BTreeMap<String, Vec<u8>>;

/// The largest single file sync moves (Overleaf's own upload limit is 50 MB).
pub const MAX_FILE: u64 = 64 * 1024 * 1024;
/// The largest whole project, unpacked.
pub const MAX_TOTAL: u64 = 1024 * 1024 * 1024;

/// Why a set of files couldn't be read.
#[derive(Debug, thiserror::Error)]
pub enum FilesError {
    /// The download wasn't a readable zip.
    #[error("the project download isn't a readable zip: {0}")]
    BadZip(String),
    /// A path in the download could escape the folder or can't be stored here.
    #[error(transparent)]
    Path(#[from] PathError),
    /// A file or the whole project is over the size limit.
    #[error("{0} is too large to sync")]
    TooLarge(String),
    /// Reading the folder failed.
    #[error("couldn't read {path}: {source}")]
    Io {
        /// What was being read.
        path: String,
        /// Why it failed.
        source: std::io::Error,
    },
}

/// A file one side couldn't read: too large, or a name some OS can't store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skipped {
    /// The project path, as that side named it.
    pub path: String,
    /// Why it was left out.
    pub why: String,
}

impl std::fmt::Display for Skipped {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.path, self.why)
    }
}

/// Files read from one side, plus the ones left out because they can't sync.
#[derive(Debug, Default)]
pub struct Loaded {
    /// The syncable files.
    pub files: Files,
    /// Files left out. Sync treats them as unchanged on that side.
    pub skipped: Vec<Skipped>,
}

fn skip(path: &str, why: impl std::fmt::Display) -> Skipped {
    Skipped {
        path: path.to_string(),
        why: why.to_string(),
    }
}

/// Unpacks a project zip from the server, leaving out ignored files and
/// skipping (never writing) unsafe paths and oversized files.
pub fn from_zip(bytes: &[u8]) -> Result<Loaded, FilesError> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| FilesError::BadZip(e.to_string()))?;
    let mut out = Loaded::default();
    let mut total = 0u64;
    for i in 0..archive.len() {
        let entry = archive
            .by_index(i)
            .map_err(|e| FilesError::BadZip(e.to_string()))?;
        if entry.is_dir() {
            continue;
        }
        let name = entry
            .name()
            .map_err(|e| FilesError::BadZip(e.to_string()))?
            .replace('\\', "/");
        // validates and normalizes; a refused path is never written anywhere
        let rel = match project_path(&name) {
            Ok(p) => to_key(&p),
            Err(e) => {
                out.skipped.push(skip(&name, e));
                continue;
            }
        };
        if ignored(&rel) {
            continue;
        }
        if entry.size() > MAX_FILE {
            out.skipped.push(skip(&rel, "larger than 64 MB"));
            continue;
        }
        let mut data = Vec::with_capacity(entry.size() as usize);
        // the header size can lie; cap the real read too
        entry
            .take(MAX_FILE + 1)
            .read_to_end(&mut data)
            .map_err(|e| FilesError::BadZip(e.to_string()))?;
        if data.len() as u64 > MAX_FILE {
            out.skipped.push(skip(&rel, "larger than 64 MB"));
            continue;
        }
        total += data.len() as u64;
        if total > MAX_TOTAL {
            return Err(FilesError::TooLarge("the project".into()));
        }
        out.files.insert(rel, data);
    }
    Ok(out)
}

fn to_key(p: &Path) -> String {
    p.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Reads every syncable file under `root`. Symlinks and junctions are
/// skipped, so a link can't pull files from outside the folder into the
/// project; names Windows can't store and oversized files are skipped too.
pub fn scan(root: &Path) -> Result<Loaded, FilesError> {
    let mut out = Loaded::default();
    let mut total = 0u64;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir).map_err(|source| FilesError::Io {
            path: dir.display().to_string(),
            source,
        })?;
        for entry in entries {
            let entry = entry.map_err(|source| FilesError::Io {
                path: dir.display().to_string(),
                source,
            })?;
            let ty = entry.file_type().map_err(|source| FilesError::Io {
                path: entry.path().display().to_string(),
                source,
            })?;
            let path = entry.path();
            let rel = to_key(path.strip_prefix(root).unwrap_or(&path));
            if ty.is_symlink() {
                continue;
            }
            if ty.is_dir() {
                // a skipped folder name anywhere in the path ignores the lot
                if !ignored(&format!("{rel}/x")) {
                    stack.push(path);
                }
                continue;
            }
            if ignored(&rel) {
                continue;
            }
            if let Err(e) = project_path(&rel) {
                out.skipped.push(skip(&rel, e));
                continue;
            }
            let len = entry.metadata().map(|m| m.len()).unwrap_or(0);
            if len > MAX_FILE {
                out.skipped.push(skip(&rel, "larger than 64 MB"));
                continue;
            }
            let data = std::fs::read(&path).map_err(|source| FilesError::Io {
                path: rel.clone(),
                source,
            })?;
            total += data.len() as u64;
            if total > MAX_TOTAL {
                return Err(FilesError::TooLarge("the folder".into()));
            }
            out.files.insert(rel, data);
        }
    }
    Ok(out)
}

/// The project path a "path: why" entry of [`Loaded::skipped`] is about.
pub fn skipped_path(entry: &str) -> &str {
    entry.rsplit_once(": ").map(|(p, _)| p).unwrap_or(entry)
}
/// Text that can be merged line by line: UTF-8 with no NUL bytes.
pub fn is_text(data: &[u8]) -> bool {
    !data.contains(&0) && std::str::from_utf8(data).is_ok()
}

/// Equal contents, treating CRLF and LF as the same in text files: Overleaf
/// stores LF, and an editor on Windows saving CRLF hasn't changed anything.
pub fn same(a: &[u8], b: &[u8]) -> bool {
    if a == b {
        return true;
    }
    if !(is_text(a) && is_text(b)) {
        return false;
    }
    normalize(a) == normalize(b)
}

/// CRLF to LF, which is how Overleaf stores docs.
pub fn normalize(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    let mut i = 0;
    while i < data.len() {
        if data[i] == b'\r' && data.get(i + 1) == Some(&b'\n') {
            i += 1;
            continue;
        }
        out.push(data[i]);
        i += 1;
    }
    out
}

/// Whether a text still has unresolved `<<<<<<<` / `>>>>>>>` conflict
/// markers, which must never be uploaded.
pub fn has_conflict_markers(data: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(data) else {
        return false;
    };
    let mut open = false;
    for line in text.lines() {
        if line.starts_with("<<<<<<< ") || line == "<<<<<<<" {
            open = true;
        } else if open && (line.starts_with(">>>>>>> ") || line == ">>>>>>>") {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn zip_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            let opts = zip::write::SimpleFileOptions::default();
            for (name, data) in entries {
                if name.ends_with('/') {
                    z.add_directory(*name, opts).unwrap();
                } else {
                    z.start_file(*name, opts).unwrap();
                    z.write_all(data).unwrap();
                }
            }
            z.finish().unwrap();
        }
        buf.into_inner()
    }

    #[test]
    fn reads_a_zip() {
        let zip = zip_of(&[
            ("main.tex", b"\\documentclass{article}"),
            ("chapters/", b""),
            ("chapters/a.tex", b"A"),
            ("main.aux", b"junk"),
        ]);
        let loaded = from_zip(&zip).unwrap();
        assert_eq!(
            loaded.files.keys().collect::<Vec<_>>(),
            vec!["chapters/a.tex", "main.tex"]
        );
        assert!(loaded.skipped.is_empty());
    }

    #[test]
    fn skips_unsafe_and_unportable_zip_entries() {
        let zip = zip_of(&[
            ("../evil.tex", b"x"),
            ("Figure 3: plot.png", b"png"),
            (".GIT/hooks/pre-commit", b"#!/bin/sh"),
            ("ok.tex", b"fine"),
        ]);
        let loaded = from_zip(&zip).unwrap();
        assert_eq!(loaded.files.keys().collect::<Vec<_>>(), vec!["ok.tex"]);
        let skipped: Vec<_> = loaded.skipped.iter().map(|s| s.path.as_str()).collect();
        assert_eq!(skipped, vec!["../evil.tex", "Figure 3: plot.png"]);
        assert!(matches!(from_zip(b"not a zip"), Err(FilesError::BadZip(_))));
    }

    #[test]
    fn scans_a_folder() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("ch").join("deep")).unwrap();
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::write(root.join("main.tex"), "x").unwrap();
        std::fs::write(root.join("ch").join("deep").join("b.tex"), "y").unwrap();
        std::fs::write(root.join(".git").join("HEAD"), "ref").unwrap();
        std::fs::write(root.join("main.log"), "log").unwrap();
        let files = scan(root).unwrap().files;
        assert_eq!(
            files.keys().collect::<Vec<_>>(),
            vec!["ch/deep/b.tex", "main.tex"]
        );
    }

    #[test]
    fn line_endings_dont_count() {
        assert!(same(b"a\r\nb\r\n", b"a\nb\n"));
        assert!(!same(b"a\nb\n", b"a\nc\n"));
        assert!(!same(&[0, 13, 10], &[0, 10]));
    }

    #[test]
    fn spots_conflict_markers() {
        assert!(has_conflict_markers(
            b"x\n<<<<<<< local\na\n=======\nb\n>>>>>>> overleaf\ny\n"
        ));
        assert!(!has_conflict_markers(b"x\n<<<<<<< only an opener\ny\n"));
        assert!(!has_conflict_markers(b"plain text\n"));
    }
}
