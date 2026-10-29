//! Files sync leaves alone: LaTeX build output, editor and OS litter, and
//! Canopy's own conflict copies. The same rules apply to all three sides, so
//! an ignored file never looks deleted.

/// File name suffixes of LaTeX build output.
const BUILD_SUFFIXES: &[&str] = &[
    ".aux",
    ".log",
    ".out",
    ".toc",
    ".lof",
    ".lot",
    ".bbl",
    ".blg",
    ".bcf",
    ".run.xml",
    ".fls",
    ".fdb_latexmk",
    ".synctex.gz",
    ".synctex",
    ".synctex(busy)",
    ".nav",
    ".snm",
    ".vrb",
    ".xdv",
    ".dvi",
    ".idx",
    ".ilg",
    ".ind",
    ".glo",
    ".gls",
    ".glg",
    ".acn",
    ".acr",
    ".alg",
    ".auxlock",
    ".figlist",
    ".makefile",
    ".brf",
    ".thm",
    ".loa",
    ".lol",
];

/// Exact file names that are OS or editor litter.
const JUNK_NAMES: &[&str] = &[".DS_Store", "Thumbs.db", "desktop.ini", ".latexmkrc.bak"];

/// Names never synced, as a folder or a file, in any letter case: `.GIT` on
/// NTFS is the same folder as `.git`, so a collaborator must not be able to
/// write into it.
const SKIP_DIRS: &[&str] = &[
    ".git", ".canopy", ".svn", ".hg", "__MACOSX", ".vscode", ".idea",
];

// a skip name, or a Windows short (8.3) alias of one such as GIT~1, which
// NTFS resolves to .git
fn skipped_name(part: &str) -> bool {
    if SKIP_DIRS.iter().any(|s| s.eq_ignore_ascii_case(part)) {
        return true;
    }
    let lower = part.to_ascii_lowercase();
    match lower.split_once('~') {
        Some((stem, n)) if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => {
            SKIP_DIRS.iter().any(|s| {
                let short: String = s.trim_start_matches('.').chars().take(6).collect();
                !short.is_empty() && stem == short.to_ascii_lowercase()
            })
        }
        _ => false,
    }
}

/// Marks the copy Canopy keeps of your side of a binary conflict, e.g.
/// `figure.local-conflict.png`. It stays on this computer only.
pub const CONFLICT_COPY: &str = ".local-conflict";

/// Whether a relative, `/`-separated path is left out of sync.
pub fn ignored(path: &str) -> bool {
    let mut parts = path.split('/').peekable();
    while let Some(part) = parts.next() {
        if skipped_name(part) {
            return true;
        }
        if parts.peek().is_some() {
            continue;
        }
        let lower = part.to_ascii_lowercase();
        return JUNK_NAMES.iter().any(|j| j.eq_ignore_ascii_case(part))
            || BUILD_SUFFIXES.iter().any(|s| lower.ends_with(s))
            || part.ends_with('~')
            || part.starts_with(".#")
            || part.starts_with("~$")
            || lower.ends_with(".swp")
            || lower.ends_with(".tmp")
            // a write sync was making when it was cut off
            || lower.ends_with(".canopy-tmp")
            || part.contains(CONFLICT_COPY);
    }
    false
}

/// The name for the local copy in a binary conflict: `a/fig.png` ->
/// `a/fig.local-conflict.png` for `n` = 1, `a/fig.local-conflict-2.png` for 2,
/// and so on, so a second conflict never overwrites the first copy.
pub fn conflict_copy_name(path: &str, n: u32) -> String {
    let (dir, name) = match path.rsplit_once('/') {
        Some((d, n)) => (format!("{d}/"), n),
        None => (String::new(), path),
    };
    let tag = if n <= 1 {
        CONFLICT_COPY.to_string()
    } else {
        format!("{CONFLICT_COPY}-{n}")
    };
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => format!("{dir}{stem}{tag}.{ext}"),
        _ => format!("{dir}{name}{tag}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_build_output_and_litter() {
        for p in [
            "main.aux",
            "main.log",
            "main.synctex.gz",
            "sub/ch1.aux",
            "main.fdb_latexmk",
            ".DS_Store",
            "fig/Thumbs.db",
            ".git/config",
            "a/.git/HEAD",
            "notes.tex~",
            ".#main.tex",
            "~$report.docx",
            "fig.local-conflict.png",
        ] {
            assert!(ignored(p), "{p} should be ignored");
        }
    }

    #[test]
    fn keeps_real_files() {
        for p in [
            "main.tex",
            "refs.bib",
            "fig/plot.pdf",
            "logo.png",
            "chapters/log.tex",
            "output/x.tex",
        ] {
            assert!(!ignored(p), "{p} should sync");
        }
    }

    #[test]
    fn names_conflict_copies() {
        assert_eq!(
            conflict_copy_name("fig/plot.png", 1),
            "fig/plot.local-conflict.png"
        );
        assert_eq!(
            conflict_copy_name("fig/plot.png", 3),
            "fig/plot.local-conflict-3.png"
        );
        assert_eq!(conflict_copy_name("Makefile", 1), "Makefile.local-conflict");
        assert_eq!(
            conflict_copy_name(".latexmkrc", 2),
            ".latexmkrc.local-conflict-2"
        );
        assert!(ignored(&conflict_copy_name("fig/plot.png", 2)));
    }

    #[test]
    fn version_control_folders_in_any_case() {
        for p in [
            ".GIT/hooks/pre-commit",
            ".Git/config",
            "sub/.GIT/x",
            ".git",
            "GIT~1/hooks/pre-commit",
            "git~2/config",
            ".VSCODE/settings.json",
            ".Idea/workspace.xml",
            "main.tex.canopy-tmp",
        ] {
            assert!(ignored(p), "{p} should be ignored");
        }
        assert!(!ignored("gitignore-notes.tex"));
        assert!(!ignored("digit~1.tex"));
        assert!(!ignored("styles.ist"));
    }
}
