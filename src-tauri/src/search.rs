// search.rs
// project search: every text file of a synced project's folder, matched
// line by line, for the Search view. ripgrep's file walker (the `ignore`
// crate) skips what sync skips, and binary files.
//
// only in the full build.

use crate::AppState;
use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tauri::{AppHandle, Manager};

// enough for any real search; a pattern like "e" stops here instead of
// sending the UI a million lines
const MAX_MATCHES: usize = 2000;
const MAX_FILE: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Query {
    text: String,
    #[serde(default)]
    regex: bool,
    #[serde(default)]
    case_sensitive: bool,
    #[serde(default)]
    whole_word: bool,
}

/// One matching line.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Hit {
    path: String,
    line: u32,
    // the line's text, and where the match is in it (char offsets)
    text: String,
    start: u32,
    end: u32,
}

#[derive(Debug, Serialize)]
pub struct Results {
    hits: Vec<Hit>,
    files: usize,
    truncated: bool,
}

fn err(e: impl std::fmt::Display) -> String {
    format!("{e:#}")
}

fn pattern(q: &Query) -> Result<Regex, String> {
    let body = if q.regex {
        q.text.clone()
    } else {
        regex::escape(&q.text)
    };
    let body = if q.whole_word {
        format!(r"\b(?:{body})\b")
    } else {
        body
    };
    RegexBuilder::new(&body)
        .case_insensitive(!q.case_sensitive)
        // a pathological pattern fails to compile instead of eating memory
        .size_limit(1 << 20)
        .build()
        .map_err(|e| format!("that isn't a valid regular expression: {e}"))
}

fn char_index(s: &str, byte: usize) -> u32 {
    s[..byte].chars().count() as u32
}

/// Searches the text files under `root`.
pub fn search(root: &Path, q: &Query) -> Result<Results, String> {
    if q.text.is_empty() {
        return Ok(Results {
            hits: Vec::new(),
            files: 0,
            truncated: false,
        });
    }
    let re = pattern(q)?;
    let mut hits = Vec::new();
    let mut files = 0;
    let walker = ignore::WalkBuilder::new(root)
        // only sync's rules: a project's .gitignore shouldn't hide files that sync
        .standard_filters(false)
        .hidden(false)
        .follow_links(false)
        .build();
    'files: for entry in walker.flatten() {
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let path = entry.path();
        let rel = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        if canopy_sync::ignore::ignored(&rel)
            || entry.metadata().map(|m| m.len()).unwrap_or(0) > MAX_FILE
        {
            continue;
        }
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        if bytes.contains(&0) {
            continue;
        }
        let Ok(text) = std::str::from_utf8(&bytes) else {
            continue;
        };
        let mut matched = false;
        for (i, line) in text.lines().enumerate() {
            if let Some(m) = re.find(line) {
                matched = true;
                hits.push(Hit {
                    path: rel.clone(),
                    line: i as u32 + 1,
                    text: line.chars().take(400).collect(),
                    start: char_index(line, m.start()),
                    end: char_index(line, m.end()),
                });
                if hits.len() >= MAX_MATCHES {
                    files += 1;
                    break 'files;
                }
            }
        }
        if matched {
            files += 1;
        }
    }
    hits.sort_by(|a, b| a.path.cmp(&b.path).then(a.line.cmp(&b.line)));
    let truncated = hits.len() >= MAX_MATCHES;
    Ok(Results {
        hits,
        files,
        truncated,
    })
}

#[tauri::command]
pub async fn search_project(
    app: AppHandle,
    link_id: String,
    query: Query,
) -> Result<Results, String> {
    let folder = {
        let state = app.state::<AppState>();
        let cfg = state.config.read().unwrap_or_else(|e| e.into_inner());
        cfg.links
            .iter()
            .find(|l| l.id == link_id)
            .map(|l| l.folder.clone())
            .ok_or("that project isn't synced to a folder")?
    };
    tauri::async_runtime::spawn_blocking(move || search(&folder, &query))
        .await
        .map_err(err)?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(text: &str) -> Query {
        Query {
            text: text.into(),
            regex: false,
            case_sensitive: false,
            whole_word: false,
        }
    }

    fn project() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let r = dir.path();
        std::fs::create_dir_all(r.join("ch")).unwrap();
        std::fs::create_dir_all(r.join(".git")).unwrap();
        std::fs::write(
            r.join("main.tex"),
            "\\section{Intro}\nSee Figure~\\ref{fig:a}.\nfigure caption\n",
        )
        .unwrap();
        std::fs::write(r.join("ch").join("a.tex"), "A figure here.\nnothing\n").unwrap();
        std::fs::write(r.join("main.log"), "figure in the log\n").unwrap();
        std::fs::write(r.join(".git").join("x"), "figure in git\n").unwrap();
        std::fs::write(r.join("img.png"), [0u8, 1, 2, b'f']).unwrap();
        // a .gitignore must not hide files sync syncs
        std::fs::write(r.join(".gitignore"), "ch/\n").unwrap();
        dir
    }

    #[test]
    fn finds_lines_across_the_project() {
        let dir = project();
        let r = search(dir.path(), &q("figure")).unwrap();
        let where_: Vec<_> = r.hits.iter().map(|h| (h.path.as_str(), h.line)).collect();
        assert_eq!(
            where_,
            vec![("ch/a.tex", 1), ("main.tex", 2), ("main.tex", 3)]
        );
        assert_eq!(r.files, 2);
        let h = &r.hits[1];
        assert_eq!(
            &h.text.chars().collect::<Vec<_>>()[h.start as usize..h.end as usize]
                .iter()
                .collect::<String>(),
            "Figure"
        );
    }

    #[test]
    fn case_words_and_regex() {
        let dir = project();
        let mut cs = q("Figure");
        cs.case_sensitive = true;
        assert_eq!(search(dir.path(), &cs).unwrap().hits.len(), 1);
        let mut word = q("fig");
        word.whole_word = true;
        // "fig:a" has fig as a word; "figure" doesn't
        assert_eq!(search(dir.path(), &word).unwrap().hits.len(), 1);
        let mut re = q(r"\\(section|ref)\{");
        re.regex = true;
        assert_eq!(search(dir.path(), &re).unwrap().hits.len(), 2);
        let mut bad = q("(");
        bad.regex = true;
        assert!(search(dir.path(), &bad).is_err());
        // a literal search treats regex characters as text
        assert!(search(dir.path(), &q("(")).unwrap().hits.is_empty());
    }

    #[test]
    fn match_offsets_count_characters() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("u.tex"), "Ünïcödé word\n").unwrap();
        let r = search(dir.path(), &q("word")).unwrap();
        assert_eq!((r.hits[0].start, r.hits[0].end), (8, 12));
    }
}
