//! Two-way sync between an Overleaf project and a folder on disk.
//!
//! The engine itself (pull, push, three-way merge against the last synced
//! snapshot) arrives in milestone 2. What is here already is the rule every
//! later piece leans on: a path that comes from the server is untrusted, and
//! must land inside the project folder or not at all.

use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

/// Why a project path from the server was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PathError {
    /// The path was empty, or only `.` and slashes.
    #[error("empty path")]
    Empty,
    /// The path tried to leave the project folder with `..`.
    #[error("path {0:?} leaves the project folder")]
    Escapes(String),
    /// The path was absolute, or carried a drive letter or UNC prefix.
    #[error("path {0:?} is absolute")]
    Absolute(String),
    /// The path contains a character or name that some OS cannot store.
    #[error("path {0:?} has a name this system can't store")]
    Unportable(String),
}

/// Where a synced project stands, shown in the sidebar and status bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SyncState {
    /// Not linked to a local folder.
    #[default]
    Off,
    /// Local and remote match.
    Synced,
    /// A pull or push is running.
    Syncing,
    /// Local edits not pushed yet (offline, or waiting to batch).
    Pending,
    /// Both sides changed the same lines; the user has to choose.
    Conflict,
    /// The last attempt failed (network, sign-in expired).
    Error,
}

/// Turns a project path as Overleaf names it (`chapters/intro.tex`, always
/// `/`-separated) into a relative path that is safe to join onto the local
/// project folder.
///
/// Refuses anything that could write outside the folder (`..`, absolute
/// paths, drive letters, UNC prefixes) and names Windows can't store, so a
/// project synced on Linux still checks out on Windows.
pub fn project_path(remote: &str) -> Result<PathBuf, PathError> {
    let raw = remote.to_string();
    if remote.starts_with('/') || remote.starts_with('\\') || has_drive(remote) {
        return Err(PathError::Absolute(raw));
    }
    let mut out = PathBuf::new();
    for part in remote.split(['/', '\\']) {
        match part {
            "" | "." => continue,
            ".." => return Err(PathError::Escapes(raw)),
            name => {
                if !portable_name(name) {
                    return Err(PathError::Unportable(raw));
                }
                out.push(name);
            }
        }
    }
    if out.as_os_str().is_empty() {
        return Err(PathError::Empty);
    }
    // belt and braces: whatever the splitting above missed, the result must be
    // made of plain names only
    debug_assert!(out.components().all(|c| matches!(c, Component::Normal(_))));
    Ok(out)
}

/// Joins a remote project path onto `root`, refusing anything [`project_path`] refuses.
pub fn resolve(root: &Path, remote: &str) -> Result<PathBuf, PathError> {
    Ok(root.join(project_path(remote)?))
}

fn has_drive(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
}

// the strictest common rules: Windows forbids these characters, control
// characters, trailing dots or spaces, and a handful of device names
fn portable_name(name: &str) -> bool {
    const BAD: &[char] = &['<', '>', ':', '"', '|', '?', '*'];
    if name.chars().any(|c| c.is_control() || BAD.contains(&c)) {
        return false;
    }
    if name.ends_with('.') || name.ends_with(' ') {
        return false;
    }
    let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[3].is_ascii_digit()
            && stem.as_bytes()[3] != b'0');
    !reserved
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_normal_paths() {
        assert_eq!(project_path("main.tex").unwrap(), PathBuf::from("main.tex"));
        assert_eq!(
            project_path("chapters/./intro.tex").unwrap(),
            Path::new("chapters").join("intro.tex")
        );
        assert_eq!(
            project_path("figs//a b.png").unwrap(),
            Path::new("figs").join("a b.png")
        );
        assert!(project_path(".latexmkrc").is_ok());
        assert!(project_path("COM0.txt").is_ok());
        assert!(project_path("console.tex").is_ok());
    }

    #[test]
    fn refuses_escapes_and_absolutes() {
        assert!(matches!(
            project_path("../evil"),
            Err(PathError::Escapes(_))
        ));
        assert!(matches!(
            project_path("a/../../evil"),
            Err(PathError::Escapes(_))
        ));
        assert!(matches!(
            project_path("a\\..\\evil"),
            Err(PathError::Escapes(_))
        ));
        assert!(matches!(
            project_path("/etc/passwd"),
            Err(PathError::Absolute(_))
        ));
        assert!(matches!(
            project_path("\\\\server\\share"),
            Err(PathError::Absolute(_))
        ));
        assert!(matches!(
            project_path("C:/Windows"),
            Err(PathError::Absolute(_))
        ));
        assert!(matches!(
            project_path("c:evil"),
            Err(PathError::Absolute(_))
        ));
    }

    #[test]
    fn refuses_empty() {
        assert_eq!(project_path(""), Err(PathError::Empty));
        assert_eq!(project_path("./."), Err(PathError::Empty));
    }

    #[test]
    fn refuses_unportable_names() {
        for bad in [
            "ab:c.tex",
            "what?.tex",
            "x\u{1}.tex",
            "trail.",
            "trail ",
            "CON",
            "nul.tex",
            "Lpt1.log",
        ] {
            assert!(
                matches!(project_path(bad), Err(PathError::Unportable(_))),
                "{bad}"
            );
        }
    }

    #[test]
    fn resolve_stays_inside_root() {
        let root = Path::new("project");
        assert_eq!(
            resolve(root, "a/b.tex").unwrap(),
            root.join("a").join("b.tex")
        );
        assert!(resolve(root, "../x").is_err());
    }
}
