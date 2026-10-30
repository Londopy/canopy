// lsp.rs
// the LaTeX language server (texlab) for the editor: one server per synced
// project, started when the editor opens it. this side only moves messages:
// the frontend's LSP client sends requests and notifications through
// commands, and gets the server's own notifications (diagnostics) as events.
// requests the server sends us (configuration, progress) are answered here.
//
// only in the full build.

use crate::AppState;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::oneshot;

type Pending = Arc<Mutex<HashMap<i64, oneshot::Sender<Value>>>>;

struct Server {
    child: Child,
    stdin: Arc<Mutex<ChildStdin>>,
    pending: Pending,
    next: AtomicI64,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[derive(Default)]
pub struct Lsp {
    servers: Mutex<HashMap<String, Arc<Server>>>,
}

fn write_message(stdin: &Mutex<ChildStdin>, msg: &Value) -> std::io::Result<()> {
    let body = serde_json::to_vec(msg)?;
    let mut w = crate::lock(stdin);
    write!(w, "Content-Length: {}\r\n\r\n", body.len())?;
    w.write_all(&body)?;
    w.flush()
}

// one framed message from the server, or None when it has gone away
fn read_message(r: &mut impl BufRead) -> Option<Value> {
    let mut len = None;
    loop {
        let mut line = String::new();
        if r.read_line(&mut line).ok()? == 0 {
            return None;
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some(v) = line.strip_prefix("Content-Length:") {
            len = v.trim().parse::<usize>().ok();
        }
    }
    // a message larger than this is a broken server, not a real reply
    let len = len.filter(|l| *l < 64 * 1024 * 1024)?;
    let mut body = vec![0; len];
    r.read_exact(&mut body).ok()?;
    serde_json::from_slice(&body).ok()
}

// what we answer the server's own requests with
fn answer(method: &str, params: &Value) -> Value {
    match method {
        // one null per item asked for: texlab's defaults are fine
        "workspace/configuration" => {
            let n = params
                .get("items")
                .and_then(|i| i.as_array())
                .map(|a| a.len())
                .unwrap_or(1);
            Value::Array(vec![Value::Null; n])
        }
        _ => Value::Null,
    }
}

fn start(app: &AppHandle, link_id: &str, root: &std::path::Path) -> Result<Arc<Server>, String> {
    let tools = canopy_texkit::find_program(
        "texlab",
        &canopy_texkit::search_path(&[crate::tools::tools_dir()]),
        &canopy_texkit::exe_extensions(),
    )
    .ok_or("texlab isn't installed")?;
    // the stand-in latexmk first on PATH: texlab must never run the real one
    // (see tools::latexmk_stub). no stand-in, no texlab
    let stub =
        crate::tools::latexmk_stub().map_err(|e| format!("couldn't set texlab up safely: {e}"))?;
    let path = std::env::join_paths(std::iter::once(stub).chain(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    )))
    .map_err(|e| e.to_string())?;
    let mut cmd = Command::new(tools);
    cmd.current_dir(root)
        .env("PATH", path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("couldn't start texlab: {e}"))?;
    let stdin = Arc::new(Mutex::new(child.stdin.take().ok_or("no stdin")?));
    let stdout = child.stdout.take().ok_or("no stdout")?;
    let pending: Pending = Arc::default();

    let (app2, id2, pending2, stdin2) = (
        app.clone(),
        link_id.to_string(),
        pending.clone(),
        stdin.clone(),
    );
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        while let Some(msg) = read_message(&mut reader) {
            let method = msg.get("method").and_then(|m| m.as_str());
            let id = msg.get("id").cloned();
            match (method, id) {
                // the server asks us something
                (Some(m), Some(id)) => {
                    let result = answer(m, msg.get("params").unwrap_or(&Value::Null));
                    let _ = write_message(
                        &stdin2,
                        &json!({"jsonrpc": "2.0", "id": id, "result": result}),
                    );
                }
                // the server tells us something (diagnostics): to the editor
                (Some(_), None) => {
                    let _ = app2.emit_to("main", "lsp", json!({"linkId": id2, "message": msg}));
                }
                // a reply to one of our requests
                (None, Some(id)) => {
                    if let Some(tx) = id.as_i64().and_then(|i| crate::lock(&pending2).remove(&i)) {
                        let _ = tx.send(msg);
                    }
                }
                _ => {}
            }
        }
        crate::dlog!("texlab for {id2} stopped");
        // wake anyone still waiting, and forget the server so the next
        // lsp_start starts a fresh one
        crate::lock(&pending2).clear();
        let servers = &app2.state::<AppState>().lsp.servers;
        let mut map = crate::lock(servers);
        if map
            .get(&id2)
            .is_some_and(|s| Arc::ptr_eq(&s.pending, &pending2))
        {
            map.remove(&id2);
        }
    });

    Ok(Arc::new(Server {
        child,
        stdin,
        pending,
        next: AtomicI64::new(1),
    }))
}

fn server(app: &AppHandle, link_id: &str) -> Result<Arc<Server>, String> {
    let state = app.state::<AppState>();
    let existing = crate::lock(&state.lsp.servers).get(link_id).cloned();
    if let Some(s) = existing {
        return Ok(s);
    }
    Err("the language server isn't running for this project".into())
}

/// Starts texlab for a synced project. Says "started" for a new server (the
/// client must initialize it), "running" when one already is (it mustn't:
/// texlab refuses a second initialize), "missing" when texlab isn't installed.
#[tauri::command]
pub async fn lsp_start(app: AppHandle, link_id: String) -> Result<String, String> {
    let state = app.state::<AppState>();
    if crate::lock(&state.lsp.servers).contains_key(&link_id) {
        return Ok("running".into());
    }
    let root = {
        let cfg = state.config.read().unwrap_or_else(|e| e.into_inner());
        cfg.links
            .iter()
            .find(|l| l.id == link_id)
            .map(|l| l.folder.clone())
            .ok_or("that project isn't synced to a folder")?
    };
    match start(&app, &link_id, &root) {
        Ok(s) => {
            crate::lock(&state.lsp.servers).insert(link_id, s);
            Ok("started".into())
        }
        Err(e) if e.contains("isn't installed") => Ok("missing".into()),
        Err(e) => Err(e),
    }
}

/// Sends a request and waits for the reply's `result` (or its error).
#[tauri::command]
pub async fn lsp_request(
    app: AppHandle,
    link_id: String,
    method: String,
    params: Value,
) -> Result<Value, String> {
    let s = server(&app, &link_id)?;
    let id = s.next.fetch_add(1, Ordering::Relaxed);
    let (tx, rx) = oneshot::channel();
    crate::lock(&s.pending).insert(id, tx);
    if let Err(e) = write_message(
        &s.stdin,
        &json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}),
    ) {
        crate::lock(&s.pending).remove(&id);
        return Err(e.to_string());
    }
    let reply = tokio::time::timeout(Duration::from_secs(20), rx)
        .await
        .map_err(|_| {
            crate::lock(&s.pending).remove(&id);
            format!("texlab didn't answer {method}")
        })?
        .map_err(|_| "texlab stopped".to_string())?;
    if let Some(e) = reply.get("error") {
        return Err(e
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("texlab error")
            .to_string());
    }
    Ok(reply.get("result").cloned().unwrap_or(Value::Null))
}

#[tauri::command]
pub async fn lsp_notify(
    app: AppHandle,
    link_id: String,
    method: String,
    params: Value,
) -> Result<(), String> {
    let s = server(&app, &link_id)?;
    write_message(
        &s.stdin,
        &json!({"jsonrpc": "2.0", "method": method, "params": params}),
    )
    .map_err(|e| e.to_string())
}

/// Stops a project's server (the editor closed it, or the project unlinked).
pub fn stop(app: &AppHandle, link_id: &str) {
    let removed = crate::lock(&app.state::<AppState>().lsp.servers).remove(link_id);
    if let Some(s) = removed {
        let _ = write_message(
            &s.stdin,
            &json!({"jsonrpc": "2.0", "id": 0, "method": "shutdown"}),
        );
        let _ = write_message(&s.stdin, &json!({"jsonrpc": "2.0", "method": "exit"}));
    }
}

#[tauri::command]
pub fn lsp_stop(app: AppHandle, link_id: String) {
    stop(&app, &link_id);
}

/// Downloads texlab (pinned version and checksum) into Canopy's tools folder.
#[tauri::command]
pub async fn install_texlab() -> Result<String, String> {
    crate::tools::install_texlab()
        .await
        .map(|p| p.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_round_trip() {
        let msg = json!({"jsonrpc": "2.0", "id": 3, "result": {"x": "ü"}});
        let body = serde_json::to_vec(&msg).unwrap();
        let mut framed = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
        framed.extend_from_slice(&body);
        framed.extend_from_slice(b"Content-Length: 2\r\nContent-Type: x\r\n\r\n{}");
        let mut r = std::io::Cursor::new(framed);
        assert_eq!(read_message(&mut r), Some(msg));
        assert_eq!(read_message(&mut r), Some(json!({})));
        assert_eq!(read_message(&mut r), None);
    }

    #[test]
    fn answers_configuration_with_one_null_per_item() {
        let a = answer("workspace/configuration", &json!({"items": [{}, {}]}));
        assert_eq!(a, json!([null, null]));
        assert_eq!(
            answer("window/workDoneProgress/create", &json!({})),
            Value::Null
        );
    }
}
