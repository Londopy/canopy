//! Deciding what a sync does, from the three versions of every file alone.
//!
//! For each path the planner looks at the base (as of the last sync), the
//! folder and the server:
//!
//! | folder vs base | server vs base | result |
//! |---|---|---|
//! | same | same | nothing |
//! | changed | same | upload (or delete on the server) |
//! | same | changed | download (or delete locally) |
//! | changed | changed, to the same thing | nothing |
//! | changed | changed differently | merge text, or keep both for binaries |
//!
//! A deletion on one side never beats an edit on the other: the edited file
//! comes back. Text with unresolved conflict markers is never uploaded.

use crate::files::{has_conflict_markers, is_text, normalize, same, Files};
use serde::Serialize;

/// One thing a sync does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Write this into the folder (a download, or a merge result).
    WriteLocal { path: String, data: Vec<u8> },
    /// Remove this from the folder.
    DeleteLocal { path: String },
    /// Send this to the server.
    Upload { path: String, data: Vec<u8> },
    /// Remove this from the server.
    DeleteRemote { path: String },
    /// Save the folder's side of a binary conflict next to `path`, under the
    /// first free `name.local-conflict[-n].ext` (the executor picks it, since
    /// earlier copies are ignored and so not in the scan). Comes right before
    /// the write that replaces `path`.
    KeepLocalCopy { path: String, data: Vec<u8> },
}

impl Step {
    /// The path the step touches.
    pub fn path(&self) -> &str {
        match self {
            Step::WriteLocal { path, .. }
            | Step::KeepLocalCopy { path, .. }
            | Step::DeleteLocal { path }
            | Step::Upload { path, .. }
            | Step::DeleteRemote { path } => path,
        }
    }

    /// Whether it changes the server (rather than the folder).
    pub fn is_remote(&self) -> bool {
        matches!(self, Step::Upload { .. } | Step::DeleteRemote { .. })
    }
}

/// How a conflict was left.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ConflictKind {
    /// Both sides edited the same lines: the folder copy now has conflict
    /// markers to resolve, and isn't uploaded until they're gone.
    Text,
    /// Both sides changed a binary file: the server's version is in place and
    /// yours is kept next to it as `name.local-conflict.ext`.
    Binary,
    /// Still has markers from an earlier sync.
    Unresolved,
}

/// A file that needs a person.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Conflict {
    /// The project path.
    pub path: String,
    /// What happened.
    pub kind: ConflictKind,
}

/// The outcome of planning.
#[derive(Debug, Clone, Default)]
pub struct Plan {
    /// What to do, in order: everything for the server first.
    pub steps: Vec<Step>,
    /// The new base once the steps are done: what both sides agree on.
    pub base: Files,
    /// Files left for the user to resolve.
    pub conflicts: Vec<Conflict>,
    /// Text files both sides changed that merged cleanly.
    pub merged: Vec<String>,
}

impl Plan {
    /// How many files the plan deletes on the server and in the folder.
    pub fn deletes(&self) -> (usize, usize) {
        let remote = self
            .steps
            .iter()
            .filter(|s| matches!(s, Step::DeleteRemote { .. }))
            .count();
        let local = self
            .steps
            .iter()
            .filter(|s| matches!(s, Step::DeleteLocal { .. }))
            .count();
        (remote, local)
    }
}

fn eq(a: Option<&Vec<u8>>, b: Option<&Vec<u8>>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => same(a, b),
        _ => false,
    }
}

// what goes to overleaf: text is stored with LF line endings
fn for_upload(data: &[u8]) -> Vec<u8> {
    if is_text(data) {
        normalize(data)
    } else {
        data.to_vec()
    }
}

/// Plans a sync. Pure: reads the three maps, touches nothing.
pub fn plan(base: &Files, local: &Files, remote: &Files) -> Plan {
    let mut out = Plan::default();
    let mut remote_steps = Vec::new();
    let mut local_steps = Vec::new();

    let paths: std::collections::BTreeSet<&String> = base
        .keys()
        .chain(local.keys())
        .chain(remote.keys())
        .collect();

    for path in paths {
        let (b, l, r) = (base.get(path), local.get(path), remote.get(path));
        let local_changed = !eq(b, l);
        let remote_changed = !eq(b, r);

        // a file still carrying markers from an earlier conflict stays put on
        // both sides until resolved, whatever else happened
        if let Some(l) = l {
            if has_conflict_markers(l) {
                out.conflicts.push(Conflict {
                    path: path.clone(),
                    kind: ConflictKind::Unresolved,
                });
                if let Some(b) = b {
                    out.base.insert(path.clone(), b.clone());
                }
                continue;
            }
        }

        match (local_changed, remote_changed) {
            (false, false) => {
                if let Some(r) = r {
                    out.base.insert(path.clone(), r.clone());
                }
            }
            (true, false) => match l {
                Some(l) => {
                    let data = for_upload(l);
                    remote_steps.push(Step::Upload {
                        path: path.clone(),
                        data: data.clone(),
                    });
                    out.base.insert(path.clone(), data);
                }
                None => remote_steps.push(Step::DeleteRemote { path: path.clone() }),
            },
            (false, true) => match r {
                Some(r) => {
                    local_steps.push(Step::WriteLocal {
                        path: path.clone(),
                        data: r.clone(),
                    });
                    out.base.insert(path.clone(), r.clone());
                }
                None => local_steps.push(Step::DeleteLocal { path: path.clone() }),
            },
            (true, true) => {
                if eq(l, r) {
                    // both made the same change
                    if let Some(r) = r {
                        out.base.insert(path.clone(), r.clone());
                    }
                    continue;
                }
                match (l, r) {
                    // deleted here, edited there: the edit wins
                    (None, Some(r)) => {
                        local_steps.push(Step::WriteLocal {
                            path: path.clone(),
                            data: r.clone(),
                        });
                        out.base.insert(path.clone(), r.clone());
                    }
                    // edited here, deleted there: the edit wins
                    (Some(l), None) => {
                        let data = for_upload(l);
                        remote_steps.push(Step::Upload {
                            path: path.clone(),
                            data: data.clone(),
                        });
                        out.base.insert(path.clone(), data);
                    }
                    (Some(l), Some(r)) => {
                        resolve_both(path, b, l, r, &mut out, &mut remote_steps, &mut local_steps)
                    }
                    (None, None) => unreachable!("eq(None, None) is handled above"),
                }
            }
        }
    }

    out.steps = remote_steps;
    out.steps.extend(local_steps);
    out
}

fn resolve_both(
    path: &str,
    base: Option<&Vec<u8>>,
    local: &[u8],
    remote: &[u8],
    out: &mut Plan,
    remote_steps: &mut Vec<Step>,
    local_steps: &mut Vec<Step>,
) {
    let base_text = base.map(|b| b.as_slice()).unwrap_or(b"");
    if is_text(local) && is_text(remote) && is_text(base_text) {
        let (b, l, r) = (normalize(base_text), normalize(local), normalize(remote));
        let (b, l, r) = (
            String::from_utf8_lossy(&b),
            String::from_utf8_lossy(&l),
            String::from_utf8_lossy(&r),
        );
        match merge(&b, &l, &r) {
            Ok(merged) => {
                let merged = merged.into_bytes();
                remote_steps.push(Step::Upload {
                    path: path.into(),
                    data: merged.clone(),
                });
                local_steps.push(Step::WriteLocal {
                    path: path.into(),
                    data: merged.clone(),
                });
                out.base.insert(path.into(), merged);
                out.merged.push(path.into());
            }
            Err(marked) => {
                // the folder gets the markers; the server keeps its version,
                // which is also the base, so the user's resolution uploads
                local_steps.push(Step::WriteLocal {
                    path: path.into(),
                    data: marked.into_bytes(),
                });
                out.base.insert(path.into(), remote.to_vec());
                out.conflicts.push(Conflict {
                    path: path.into(),
                    kind: ConflictKind::Text,
                });
            }
        }
    } else {
        // can't merge bytes: keep ours alongside first, then take the server's
        local_steps.push(Step::KeepLocalCopy {
            path: path.into(),
            data: local.to_vec(),
        });
        local_steps.push(Step::WriteLocal {
            path: path.into(),
            data: remote.to_vec(),
        });
        out.base.insert(path.into(), remote.to_vec());
        out.conflicts.push(Conflict {
            path: path.into(),
            kind: ConflictKind::Binary,
        });
    }
}

/// A three-way line merge. `Err` carries the text with conflict markers, the
/// folder's side first and Overleaf's second.
pub fn merge(base: &str, local: &str, remote: &str) -> Result<String, String> {
    // diffy works line by line; a last line without a newline would glue the
    // closing markers onto the text (and slip past has_conflict_markers), so
    // every side gets one, and it comes off again when neither edit had it
    let end = |s: &str| {
        if s.is_empty() || s.ends_with('\n') {
            s.to_string()
        } else {
            format!("{s}\n")
        }
    };
    let keep_newline = local.ends_with('\n') || remote.ends_with('\n');
    let trim = |s: String| {
        if !keep_newline && s.ends_with('\n') {
            s[..s.len() - 1].to_string()
        } else {
            s
        }
    };
    let mut opts = diffy::MergeOptions::new();
    opts.set_conflict_style(diffy::ConflictStyle::Merge);
    match opts.merge(&end(base), &end(local), &end(remote)) {
        Ok(merged) => Ok(trim(merged)),
        Err(marked) => Err(relabel(&marked)),
    }
}

// diffy writes "<<<<<<< ours" / ">>>>>>> theirs"; say which side is which
fn relabel(text: &str) -> String {
    text.lines()
        .map(|line| match line {
            "<<<<<<< ours" => "<<<<<<< your folder",
            ">>>>>>> theirs" => ">>>>>>> overleaf",
            other => other,
        })
        .collect::<Vec<_>>()
        .join("\n")
        + if text.ends_with('\n') { "\n" } else { "" }
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

    fn steps(p: &Plan) -> Vec<String> {
        p.steps
            .iter()
            .map(|s| match s {
                Step::WriteLocal { path, .. } => format!("write {path}"),
                Step::KeepLocalCopy { path, .. } => format!("keep {path}"),
                Step::DeleteLocal { path } => format!("rm {path}"),
                Step::Upload { path, .. } => format!("upload {path}"),
                Step::DeleteRemote { path } => format!("rm-remote {path}"),
            })
            .collect()
    }

    #[test]
    fn nothing_changed() {
        let f = files(&[("a.tex", "x\n")]);
        let p = plan(&f, &f, &f);
        assert!(p.steps.is_empty() && p.conflicts.is_empty());
        assert_eq!(p.base, f);
    }

    #[test]
    fn one_sided_changes() {
        let base = files(&[
            ("a.tex", "a\n"),
            ("b.tex", "b\n"),
            ("c.tex", "c\n"),
            ("d.tex", "d\n"),
        ]);
        let local = files(&[
            ("a.tex", "A\n"),
            ("c.tex", "c\n"),
            ("d.tex", "d\n"),
            ("new.tex", "n\n"),
        ]);
        let remote = files(&[
            ("a.tex", "a\n"),
            ("b.tex", "b\n"),
            ("c.tex", "C\n"),
            ("r.tex", "r\n"),
        ]);
        let p = plan(&base, &local, &remote);
        assert_eq!(
            steps(&p),
            vec![
                "upload a.tex",
                "rm-remote b.tex",
                "upload new.tex",
                "write c.tex",
                "rm d.tex",
                "write r.tex"
            ]
        );
        assert_eq!(
            p.base,
            files(&[
                ("a.tex", "A\n"),
                ("c.tex", "C\n"),
                ("new.tex", "n\n"),
                ("r.tex", "r\n")
            ])
        );
    }

    #[test]
    fn first_sync_into_an_empty_folder_pulls_everything() {
        let remote = files(&[("main.tex", "m\n"), ("fig/a.png", "png")]);
        let p = plan(&Files::new(), &Files::new(), &remote);
        assert_eq!(steps(&p), vec!["write fig/a.png", "write main.tex"]);
        assert_eq!(p.base, remote);
    }

    #[test]
    fn line_endings_alone_are_not_a_change() {
        let base = files(&[("a.tex", "x\ny\n")]);
        let local = files(&[("a.tex", "x\r\ny\r\n")]);
        let p = plan(&base, &local, &base);
        assert!(p.steps.is_empty());
    }

    #[test]
    fn uploads_text_with_lf() {
        let base = files(&[("a.tex", "x\n")]);
        let local = files(&[("a.tex", "x\r\nmore\r\n")]);
        let p = plan(&base, &local, &base);
        assert_eq!(
            p.steps,
            vec![Step::Upload {
                path: "a.tex".into(),
                data: b"x\nmore\n".to_vec()
            }]
        );
    }

    #[test]
    fn merges_edits_to_different_lines() {
        let base = files(&[("a.tex", "one\ntwo\nthree\n")]);
        let local = files(&[("a.tex", "ONE\ntwo\nthree\n")]);
        let remote = files(&[("a.tex", "one\ntwo\nTHREE\n")]);
        let p = plan(&base, &local, &remote);
        assert_eq!(p.merged, vec!["a.tex"]);
        assert_eq!(p.base["a.tex"], b"ONE\ntwo\nTHREE\n");
        assert_eq!(steps(&p), vec!["upload a.tex", "write a.tex"]);
    }

    #[test]
    fn conflicting_lines_get_markers_and_stay_local() {
        let base = files(&[("a.tex", "one\n")]);
        let local = files(&[("a.tex", "mine\n")]);
        let remote = files(&[("a.tex", "theirs\n")]);
        let p = plan(&base, &local, &remote);
        assert_eq!(
            p.conflicts,
            vec![Conflict {
                path: "a.tex".into(),
                kind: ConflictKind::Text
            }]
        );
        assert_eq!(steps(&p), vec!["write a.tex"]);
        let Step::WriteLocal { data, .. } = &p.steps[0] else {
            panic!()
        };
        let text = String::from_utf8(data.clone()).unwrap();
        assert!(
            text.contains("<<<<<<< your folder") && text.contains(">>>>>>> overleaf"),
            "{text}"
        );
        assert_eq!(p.base["a.tex"], b"theirs\n");

        // next sync, still unresolved: nothing is uploaded
        let p2 = plan(&p.base, &files(&[("a.tex", &text)]), &remote);
        assert!(p2.steps.is_empty());
        assert_eq!(p2.conflicts[0].kind, ConflictKind::Unresolved);

        // resolved: the fix uploads
        let p3 = plan(&p.base, &files(&[("a.tex", "settled\n")]), &remote);
        assert_eq!(steps(&p3), vec!["upload a.tex"]);
    }

    #[test]
    fn binary_conflicts_keep_both() {
        let base: Files = [("fig.png".to_string(), vec![0u8, 1])].into();
        let local: Files = [("fig.png".to_string(), vec![0u8, 2])].into();
        let remote: Files = [("fig.png".to_string(), vec![0u8, 3])].into();
        let p = plan(&base, &local, &remote);
        assert_eq!(steps(&p), vec!["keep fig.png", "write fig.png"]);
        assert_eq!(p.conflicts[0].kind, ConflictKind::Binary);
        assert_eq!(p.base["fig.png"], vec![0u8, 3]);
    }

    #[test]
    fn last_lines_without_newlines_still_merge_or_mark() {
        // both sides change the final \end{document} line differently
        let r = merge(
            "a\n\\end{document}",
            "A\n\\end{document} %x",
            "a\n\\end{document} %y",
        );
        let marked = r.unwrap_err();
        assert!(has_conflict_markers(marked.as_bytes()), "{marked}");

        // different lines, neither ends in a newline: merges, and stays without one
        let ok = merge("one\nmid\ntwo", "ONE\nmid\ntwo", "one\nmid\nTWO").unwrap();
        assert_eq!(ok, "ONE\nmid\nTWO");
        // one side added the newline: kept
        assert_eq!(
            merge("one\nmid\ntwo", "ONE\nmid\ntwo\n", "one\nmid\ntwo").unwrap(),
            "ONE\nmid\ntwo\n"
        );
    }

    #[test]
    fn edits_beat_deletes() {
        let base = files(&[("a.tex", "a\n"), ("b.tex", "b\n")]);
        let local = files(&[("b.tex", "B local\n")]); // a deleted here, b edited
        let remote = files(&[("a.tex", "A remote\n")]); // a edited there, b deleted
        let p = plan(&base, &local, &remote);
        assert_eq!(steps(&p), vec!["upload b.tex", "write a.tex"]);
        assert_eq!(
            p.base,
            files(&[("a.tex", "A remote\n"), ("b.tex", "B local\n")])
        );
    }

    #[test]
    fn same_change_on_both_sides_is_quiet() {
        let base = files(&[("a.tex", "a\n")]);
        let both = files(&[("b.tex", "same\n")]);
        let p = plan(&base, &both, &both);
        assert!(p.steps.is_empty());
        assert_eq!(p.base, both);
        assert_eq!(p.deletes(), (0, 0));
    }

    #[test]
    fn added_on_both_sides_differently_conflicts() {
        let local = files(&[("x.tex", "mine\n")]);
        let remote = files(&[("x.tex", "theirs\n")]);
        let p = plan(&Files::new(), &local, &remote);
        assert_eq!(p.conflicts.len(), 1);
    }
}
