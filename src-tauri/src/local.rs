// local.rs
// local mode: editing a synced project's folder in Canopy, compiling it with
// the TeX install on this computer and jumping between source and PDF.
// saving into the folder is what the sync watcher picks up, so edits made
// here reach Overleaf the same way edits made in any other editor do.
//
// only in the full build.

use crate::config::{self, SyncLink};
use crate::{accounts, AppState};
use canopy_texkit::compile::{self, Job, Outcome};
use canopy_texkit::synctex::{Spot, Synctex};
use canopy_texkit::{search_path, Toolchain};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};

// a document that hasn't finished in this long has hung
const COMPILE_TIMEOUT: Duration = Duration::from_secs(300);
// files larger than this don't open in the editor
const MAX_EDIT: u64 = 8 * 1024 * 1024;

#[derive(Default)]
pub struct Local {
    // the TeX tools found on this computer, looked up once
    tools: tokio::sync::OnceCell<Toolchain>,
    // per link: the running compile's cancel flag, and the last result
    cancel: Mutex<HashMap<String, Arc<AtomicBool>>>,
    last: Mutex<HashMap<String, Outcome>>,
}

fn err(e: impl std::fmt::Display) -> String {
    format!("{e:#}")
}

fn link(app: &AppHandle, id: &str) -> Result<SyncLink, String> {
    if !accounts::valid_id(id) {
        return Err("bad link id".into());
    }
    let state = app.state::<AppState>();
    let cfg = state.config.read().unwrap_or_else(|e| e.into_inner());
    cfg.links
        .iter()
        .find(|l| l.id == id)
        .cloned()
        .ok_or_else(|| "that project isn't synced to a folder".into())
}

fn build_dir(link_id: &str) -> PathBuf {
    config::data_dir().join("build").join(link_id)
}

/// The TeX tools, found the first time they're needed. Detection can take a
/// few seconds (it checks that latexmk really runs), so it happens once.
pub async fn tools(app: &AppHandle) -> Toolchain {
    let state = app.state::<AppState>();
    state
        .local
        .tools
        .get_or_init(|| async {
            let own = vec![config::data_dir().join("tools")];
            tauri::async_runtime::spawn_blocking(move || Toolchain::detect(&own))
                .await
                .unwrap_or_default()
        })
        .await
        .clone()
}

/// One entry of the file tree.
#[derive(Serialize)]
pub struct FileEntry {
    path: String,
    size: u64,
}

/// Every file sync would see in the project folder (no build output, no
/// `.git`, no conflict copies), sorted by path.
#[tauri::command]
pub async fn local_files(app: AppHandle, link_id: String) -> Result<Vec<FileEntry>, String> {
    let l = link(&app, &link_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut out = Vec::new();
        let mut stack = vec![l.folder.clone()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for e in entries.flatten() {
                let Ok(ty) = e.file_type() else { continue };
                if ty.is_symlink() {
                    continue;
                }
                let p = e.path();
                let rel = p
                    .strip_prefix(&l.folder)
                    .unwrap_or(&p)
                    .to_string_lossy()
                    .replace('\\', "/");
                if ty.is_dir() {
                    if !canopy_sync::ignore::ignored(&format!("{rel}/x")) {
                        stack.push(p);
                    }
                } else if !canopy_sync::ignore::ignored(&rel) {
                    let size = e.metadata().map(|m| m.len()).unwrap_or(0);
                    out.push(FileEntry { path: rel, size });
                }
            }
        }
        out.sort_by_key(|a| a.path.to_lowercase());
        out
    })
    .await
    .map_err(err)
}

fn target(l: &SyncLink, path: &str) -> Result<PathBuf, String> {
    canopy_sync::resolve(&l.folder, path).map_err(err)
}

/// A text file of the project, for the editor.
#[tauri::command]
pub async fn read_text(app: AppHandle, link_id: String, path: String) -> Result<String, String> {
    let l = link(&app, &link_id)?;
    let t = target(&l, &path)?;
    let meta = std::fs::symlink_metadata(&t).map_err(err)?;
    if meta.file_type().is_symlink() {
        return Err(format!("{path} is a link"));
    }
    if meta.len() > MAX_EDIT {
        return Err(format!("{path} is too large to edit here"));
    }
    let bytes = std::fs::read(&t).map_err(err)?;
    if bytes.contains(&0) {
        return Err(format!("{path} isn't a text file"));
    }
    String::from_utf8(bytes).map_err(|_| format!("{path} isn't UTF-8 text"))
}

/// What a save wrote.
#[derive(Serialize)]
pub struct Saved {
    // the text now in the file: the editor's, or a merge of it with a change
    // that reached the file since the editor loaded it
    text: String,
    merged: bool,
}

/// Saves a text file into the project folder; sync uploads it from there.
///
/// `base` is the text the editor loaded (or last saved). When the file has
/// changed since then (a sync brought someone's edit), the two are merged
/// line by line rather than the editor's copy overwriting theirs; when both
/// changed the same lines, the save is refused with an error starting
/// "conflict:" unless `force` is set.
#[tauri::command]
pub async fn write_text(
    app: AppHandle,
    link_id: String,
    path: String,
    text: String,
    base: Option<String>,
    force: bool,
) -> Result<Saved, String> {
    let l = link(&app, &link_id)?;
    if canopy_sync::ignore::ignored(&path) {
        return Err(format!("{path} is a name Canopy doesn't sync"));
    }
    let t = target(&l, &path)?;
    tauri::async_runtime::spawn_blocking(move || {
        let disk = std::fs::read(&t)
            .ok()
            .map(|b| String::from_utf8_lossy(&b).into_owned());
        let mut out = Saved {
            text: text.clone(),
            merged: false,
        };
        if let (false, Some(base), Some(disk)) = (force, base, disk.as_ref()) {
            let same = |a: &str, b: &str| canopy_sync::files::same(a.as_bytes(), b.as_bytes());
            if !same(disk, &base) {
                if same(disk, &text) {
                    return Ok(out);
                }
                match canopy_sync::plan::merge(&base, &text, disk) {
                    Ok(m) => {
                        out = Saved {
                            text: m,
                            merged: true,
                        }
                    }
                    Err(_) => {
                        return Err(format!(
                            "conflict: {path} changed on disk (probably a sync) on the same lines you edited"
                        ))
                    }
                }
            }
        }
        canopy_sync::write_project_file(&l.folder, &path, out.text.as_bytes()).map_err(err)?;
        Ok(out)
    })
    .await
    .map_err(err)?
}
/// Makes a new, empty file (a new chapter, say). Refuses to overwrite.
#[tauri::command]
pub async fn create_text(app: AppHandle, link_id: String, path: String) -> Result<(), String> {
    let l = link(&app, &link_id)?;
    if canopy_sync::ignore::ignored(&path) {
        return Err(format!("{path} is a name Canopy doesn't sync"));
    }
    if target(&l, &path)?.exists() {
        return Err(format!("{path} already exists"));
    }
    canopy_sync::write_project_file(&l.folder, &path, b"").map_err(err)
}

/// The main file a compile builds: the one chosen for this project, or a
/// guess (main.tex, then a top-level file with \documentclass).
#[tauri::command]
pub async fn main_file(app: AppHandle, link_id: String) -> Result<Option<String>, String> {
    let l = link(&app, &link_id)?;
    if let Some(m) = l
        .main_file
        .filter(|m| compile::safe_main(m) && l.folder.join(m).is_file())
    {
        return Ok(Some(m));
    }
    let files = local_files(app, link_id).await?;
    let tex: Vec<String> = files
        .into_iter()
        .map(|f| f.path)
        .filter(|p| p.to_ascii_lowercase().ends_with(".tex"))
        .collect();
    Ok(compile::find_main(&l.folder, &tex))
}

#[tauri::command]
pub async fn set_main_file(app: AppHandle, link_id: String, path: String) -> Result<(), String> {
    let l = link(&app, &link_id)?;
    if !compile::safe_main(&path) {
        return Err(format!("{path} can't be the main file"));
    }
    if !target(&l, &path)?.is_file() {
        return Err(format!("{path} doesn't exist"));
    }
    let state = app.state::<AppState>();
    crate::change_config(&state, |c| {
        if let Some(x) = c.links.iter_mut().find(|x| x.id == link_id) {
            x.main_file = Some(path.clone());
        }
        Ok(())
    })?;
    Ok(())
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CompileEvent {
    link_id: String,
    running: bool,
}

/// Compiles the project with the TeX install on this computer. One compile
/// per project at a time: a second request while one runs is refused.
#[tauri::command]
pub async fn compile_project(app: AppHandle, link_id: String) -> Result<Outcome, String> {
    let l = link(&app, &link_id)?;
    let main = main_file(app.clone(), link_id.clone())
        .await?
        .ok_or("no main file: none of the .tex files has \\documentclass")?;
    let tc = tools(&app).await;
    let pref = app
        .state::<AppState>()
        .config
        .read()
        .map(|c| c.compile.engine)
        .unwrap_or_default();
    let engine = tc.choose(pref).ok_or(
        "no TeX compiler found. Install TeX Live or MiKTeX, or put tectonic on PATH, then restart Canopy",
    )?;

    let flag = Arc::new(AtomicBool::new(false));
    {
        let state = app.state::<AppState>();
        let mut running = crate::lock(&state.local.cancel);
        if running.contains_key(&link_id) {
            return Err("this project is already compiling".into());
        }
        running.insert(link_id.clone(), flag.clone());
    }
    let _ = app.emit_to(
        "main",
        "compile",
        CompileEvent {
            link_id: link_id.clone(),
            running: true,
        },
    );
    let job = Job {
        engine,
        search: search_path(&[config::data_dir().join("tools")]),
        root: l.folder.clone(),
        main,
        out_dir: build_dir(&link_id),
        timeout: COMPILE_TIMEOUT,
    };
    let result = tauri::async_runtime::spawn_blocking(move || compile::compile(&job, &flag))
        .await
        .map_err(err)
        .and_then(|r| r.map_err(err));

    let state = app.state::<AppState>();
    crate::lock(&state.local.cancel).remove(&link_id);
    let _ = app.emit_to(
        "main",
        "compile",
        CompileEvent {
            link_id: link_id.clone(),
            running: false,
        },
    );
    if let Ok(o) = &result {
        crate::dlog!(
            "compiled {} with {:?} in {:.1}s: ok={} ({} problems)",
            l.project_name,
            o.engine,
            o.seconds,
            o.ok,
            o.diagnostics.len()
        );
        // a stopped compile left a cut-off PDF: the last good result stays
        if !o.cancelled && !o.timed_out {
            crate::lock(&state.local.last).insert(link_id, o.clone());
        }
    }
    result
}

/// Stops every running compile, for when Canopy quits: otherwise the TeX
/// programs it started would keep running. Waits briefly for them to go.
pub fn cancel_all(app: &AppHandle) {
    let state = app.state::<AppState>();
    for flag in crate::lock(&state.local.cancel).values() {
        flag.store(true, Ordering::Relaxed);
    }
    for _ in 0..40 {
        if crate::lock(&state.local.cancel).is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[tauri::command]
pub fn cancel_compile(state: State<'_, AppState>, link_id: String) {
    if let Some(flag) = crate::lock(&state.local.cancel).get(&link_id) {
        flag.store(true, Ordering::Relaxed);
    }
}

fn last(app: &AppHandle, link_id: &str) -> Result<Outcome, String> {
    crate::lock(&app.state::<AppState>().local.last)
        .get(link_id)
        .cloned()
        .ok_or_else(|| "compile the project first".into())
}

/// The last compile's PDF, as raw bytes for the PDF view.
#[tauri::command]
pub async fn read_pdf(app: AppHandle, link_id: String) -> Result<tauri::ipc::Response, String> {
    let pdf = last(&app, &link_id)?
        .pdf
        .ok_or("the last compile made no PDF")?;
    // only ever the build folder's PDF
    if !pdf.starts_with(build_dir(&link_id)) {
        return Err("unexpected PDF location".into());
    }
    let bytes = tauri::async_runtime::spawn_blocking(move || std::fs::read(pdf))
        .await
        .map_err(err)?
        .map_err(err)?;
    Ok(tauri::ipc::Response::new(bytes))
}

fn synctex_for(app: &AppHandle, link_id: &str) -> Result<Synctex, String> {
    let file = last(app, link_id)?
        .synctex
        .ok_or("the last compile wrote no SyncTeX file")?;
    Synctex::load(&file).map_err(err)
}

/// Where a line of a project file is in the PDF.
#[tauri::command]
pub async fn synctex_forward(
    app: AppHandle,
    link_id: String,
    path: String,
    line: u32,
) -> Result<Vec<Spot>, String> {
    let l = link(&app, &link_id)?;
    let sync = synctex_for(&app, &link_id)?;
    Ok(sync.forward(&target(&l, &path)?, line))
}

/// The project file and line behind a point in the PDF.
#[derive(Serialize)]
pub struct SourceSpot {
    path: String,
    line: u32,
}

#[tauri::command]
pub async fn synctex_backward(
    app: AppHandle,
    link_id: String,
    page: u32,
    x: f64,
    y: f64,
) -> Result<Option<SourceSpot>, String> {
    let l = link(&app, &link_id)?;
    let sync = synctex_for(&app, &link_id)?;
    let Some(hit) = sync.backward(page, x, y) else {
        return Ok(None);
    };
    Ok(relative_to(&l.folder, &hit.file).map(|path| SourceSpot {
        path,
        line: hit.line,
    }))
}

// a path SyncTeX recorded, as a project path; None for files outside the
// project (a class or package file)
fn relative_to(root: &Path, file: &Path) -> Option<String> {
    let norm = |p: &Path| {
        let s = p.to_string_lossy().replace('\\', "/");
        let s = s.strip_prefix("./").unwrap_or(&s).to_string();
        if cfg!(windows) {
            s.to_lowercase()
        } else {
            s
        }
    };
    let (r, f) = (norm(root), norm(file));
    let original = file.to_string_lossy().replace('\\', "/");
    if file.is_relative() {
        return Some(original.strip_prefix("./").unwrap_or(&original).to_string());
    }
    let prefix = format!("{}/", r.trim_end_matches('/'));
    f.starts_with(&prefix)
        .then(|| original[prefix.len()..].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synctex_paths_become_project_paths() {
        let root = Path::new("C:\\proj");
        assert_eq!(
            relative_to(root, Path::new("C:\\proj\\chapters\\one.tex")).as_deref(),
            Some("chapters/one.tex")
        );
        assert_eq!(
            relative_to(root, Path::new("./main.tex")).as_deref(),
            Some("main.tex")
        );
        if cfg!(windows) {
            assert_eq!(
                relative_to(root, Path::new("c:\\PROJ\\Main.tex")).as_deref(),
                Some("Main.tex")
            );
            assert!(relative_to(root, Path::new("C:\\texmf\\article.cls")).is_none());
        }
    }
}
