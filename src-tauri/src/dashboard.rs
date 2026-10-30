// dashboard.rs
// the writing dashboard: words per section across a project (following
// \input and \include from the main file), words written per day, and a
// goal with a deadline. the daily history is a small JSON file per project
// in canopy's data folder; nothing here touches the project or the server.
//
// only in the full build.

use crate::{config, AppState};
use canopy_texkit::words::{self, Section};
use chrono::{Duration, Local, NaiveDate};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use tauri::{AppHandle, Manager};

// how many days of history the file keeps
const KEEP_DAYS: i64 = 400;

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct Goals {
    // total words to reach, 0 = none
    #[serde(default)]
    pub target: u32,
    // the day it's due, YYYY-MM-DD
    #[serde(default)]
    pub deadline: Option<String>,
    // words to write each day, 0 = work it out from target and deadline
    #[serde(default)]
    pub daily: u32,
}

// what's stored per project: the goals, and the project's total at the end
// of each day it was looked at
#[derive(Debug, Default, Serialize, Deserialize)]
struct Store {
    #[serde(default)]
    goals: Goals,
    #[serde(default)]
    totals: BTreeMap<String, u32>,
}

#[derive(Debug, Serialize)]
pub struct FileCount {
    path: String,
    words: u32,
    sections: Vec<Section>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Dashboard {
    total: u32,
    formulas: u32,
    files: Vec<FileCount>,
    // (day, words written that day), oldest first, last 30 days
    days: Vec<(String, i64)>,
    today: i64,
    goals: Goals,
    // words a day still needed to meet the deadline, when there is one
    needed_per_day: Option<u32>,
    days_left: Option<i64>,
    main: Option<String>,
}

static INCLUDE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\\(?:input|include|subfile)\s*\{([^}]+)\}").expect("regex"));

fn err(e: impl std::fmt::Display) -> String {
    format!("{e:#}")
}

fn store_path(link_id: &str) -> PathBuf {
    config::data_dir()
        .join("dashboard")
        .join(format!("{link_id}.json"))
}

fn load(link_id: &str) -> Store {
    std::fs::read_to_string(store_path(link_id))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn save(link_id: &str, s: &Store) -> Result<(), String> {
    let p = store_path(link_id);
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d).map_err(err)?;
    }
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(s).map_err(err)?).map_err(err)?;
    std::fs::rename(&tmp, &p).map_err(err)
}

/// The .tex files a main file pulls in, in reading order, each once; paths
/// that leave the folder are skipped.
pub fn reachable(root: &Path, main: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut stack = vec![main.to_string()];
    while let Some(file) = stack.pop() {
        if out.contains(&file) || out.len() > 500 {
            continue;
        }
        let Ok(target) = canopy_sync::resolve(root, &file) else {
            continue;
        };
        let Ok(src) = std::fs::read_to_string(&target) else {
            continue;
        };
        out.push(file.clone());
        // \input paths are relative to the project root, as on Overleaf
        let mut found: Vec<String> = INCLUDE
            .captures_iter(&strip_comments(&src))
            .map(|c| {
                let name = c[1].trim().replace('\\', "/");
                if name.to_ascii_lowercase().ends_with(".tex") {
                    name
                } else {
                    format!("{name}.tex")
                }
            })
            .collect();
        found.reverse();
        stack.extend(found);
    }
    out
}

fn strip_comments(src: &str) -> String {
    src.lines()
        .map(|l| {
            let mut prev = ' ';
            for (i, c) in l.char_indices() {
                if c == '%' && prev != '\\' {
                    return &l[..i];
                }
                prev = c;
            }
            l
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Words written per day from end-of-day totals: each day's total minus the
/// day before's. A day with no record counts as no change.
pub fn per_day(totals: &BTreeMap<String, u32>, today: NaiveDate, days: i64) -> Vec<(String, i64)> {
    let mut out = Vec::new();
    for back in (0..days).rev() {
        let day = today - Duration::days(back);
        let key = day.format("%Y-%m-%d").to_string();
        let total = |d: NaiveDate| -> Option<u32> {
            // the latest total on or before d
            totals
                .range(..=d.format("%Y-%m-%d").to_string())
                .next_back()
                .map(|(_, v)| *v)
        };
        let written = match (total(day), total(day - Duration::days(1))) {
            (Some(now), Some(before)) if totals.contains_key(&key) => now as i64 - before as i64,
            // the very first day on record: nothing to compare with
            _ => 0,
        };
        out.push((key, written));
    }
    out
}

/// Words a day still needed to reach `target` by `deadline` (inclusive),
/// from `total` now.
pub fn needed(
    total: u32,
    target: u32,
    today: NaiveDate,
    deadline: NaiveDate,
) -> (Option<u32>, i64) {
    let left = (deadline - today).num_days() + 1;
    if target == 0 || total >= target {
        return (Some(0), left);
    }
    if left <= 0 {
        return (None, left);
    }
    (Some((target - total).div_ceil(left as u32)), left)
}

#[tauri::command]
pub async fn dashboard(app: AppHandle, link_id: String) -> Result<Dashboard, String> {
    let folder = {
        let state = app.state::<AppState>();
        let cfg = state.config.read().unwrap_or_else(|e| e.into_inner());
        cfg.links
            .iter()
            .find(|l| l.id == link_id)
            .map(|l| l.folder.clone())
            .ok_or("that project isn't synced to a folder")?
    };
    let main = crate::local::main_file(app.clone(), link_id.clone()).await?;
    tauri::async_runtime::spawn_blocking(move || {
        let files_to_count = match &main {
            Some(m) => reachable(&folder, m),
            None => Vec::new(),
        };
        let mut files = Vec::new();
        let (mut total, mut formulas) = (0, 0);
        for f in files_to_count {
            let Ok(target) = canopy_sync::resolve(&folder, &f) else {
                continue;
            };
            let Ok(src) = std::fs::read_to_string(target) else {
                continue;
            };
            let c = words::count(&src);
            total += c.words;
            formulas += c.formulas;
            files.push(FileCount {
                path: f,
                words: c.words,
                sections: c.sections,
            });
        }

        let mut store = load(&link_id);
        let today = Local::now().date_naive();
        // no main file means nothing was counted, not that the words are gone:
        // recording 0 would show a fake drop and a fake spike after it
        if main.is_some() {
            store
                .totals
                .insert(today.format("%Y-%m-%d").to_string(), total);
        }
        let cutoff = (today - Duration::days(KEEP_DAYS))
            .format("%Y-%m-%d")
            .to_string();
        store.totals.retain(|d, _| *d >= cutoff);
        save(&link_id, &store)?;

        let days = per_day(&store.totals, today, 30);
        let today_words = days.last().map(|(_, w)| *w).unwrap_or(0);
        let deadline = store
            .goals
            .deadline
            .as_deref()
            .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok());
        let (needed_per_day, days_left) = match deadline {
            Some(d) => {
                // from where the day started: the words written today count
                // toward today's share, not against it
                let start_of_day = (total as i64 - today_words).max(0) as u32;
                let (n, left) = needed(start_of_day, store.goals.target, today, d);
                (n, Some(left))
            }
            None => (None, None),
        };
        Ok(Dashboard {
            total,
            formulas,
            files,
            days,
            today: today_words,
            goals: store.goals.clone(),
            needed_per_day,
            days_left,
            main,
        })
    })
    .await
    .map_err(err)?
}

#[tauri::command]
pub fn set_goals(link_id: String, goals: Goals) -> Result<(), String> {
    if !crate::accounts::valid_id(&link_id) {
        return Err("bad link id".into());
    }
    if let Some(d) = &goals.deadline {
        NaiveDate::parse_from_str(d, "%Y-%m-%d").map_err(|_| "the deadline isn't a date")?;
    }
    let mut store = load(&link_id);
    store.goals = goals;
    save(&link_id, &store)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn follows_inputs_from_the_main_file() {
        let dir = tempfile::tempdir().unwrap();
        let r = dir.path();
        std::fs::create_dir_all(r.join("ch")).unwrap();
        std::fs::write(r.join("main.tex"), "\\input{ch/one}\n% \\input{ch/commented}\n\\include{ch/two.tex}\n\\input{../outside}\n\\input{ch/one}").unwrap();
        std::fs::write(r.join("ch/one.tex"), "\\input{ch/deep}").unwrap();
        std::fs::write(r.join("ch/deep.tex"), "deep").unwrap();
        std::fs::write(r.join("ch/two.tex"), "two").unwrap();
        std::fs::write(r.join("ch/commented.tex"), "no").unwrap();
        assert_eq!(
            reachable(r, "main.tex"),
            vec!["main.tex", "ch/one.tex", "ch/deep.tex", "ch/two.tex"]
        );
    }

    #[test]
    fn words_per_day_from_totals() {
        let totals: BTreeMap<String, u32> = [
            ("2026-10-20".to_string(), 1000),
            ("2026-10-22".to_string(), 1300),
            ("2026-10-23".to_string(), 1250),
        ]
        .into();
        let days = per_day(&totals, d("2026-10-23"), 5);
        assert_eq!(
            days,
            vec![
                ("2026-10-19".into(), 0),
                ("2026-10-20".into(), 0),
                ("2026-10-21".into(), 0),
                ("2026-10-22".into(), 300),
                ("2026-10-23".into(), -50),
            ]
        );
    }

    #[test]
    fn words_needed_by_the_deadline() {
        // 4000 to go over 4 days (today through the 26th)
        assert_eq!(
            needed(6000, 10000, d("2026-10-23"), d("2026-10-26")),
            (Some(1000), 4)
        );
        assert_eq!(
            needed(12000, 10000, d("2026-10-23"), d("2026-10-26")).0,
            Some(0)
        );
        // past the deadline
        assert_eq!(
            needed(6000, 10000, d("2026-10-27"), d("2026-10-26")),
            (None, 0)
        );
    }
}
