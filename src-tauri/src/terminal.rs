// terminal.rs
// a real terminal in the editor's bottom panel: your shell (PowerShell on
// Windows, $SHELL elsewhere) in a pseudo-terminal, started in the project's
// folder. output goes to the main window only, as events; keystrokes come
// back through commands.
//
// only in the full build.

use crate::AppState;
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

const MAX_TERMINALS: usize = 8;

type ChildBox = Box<dyn portable_pty::Child + Send + Sync>;

struct Term {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    // shared with the thread that waits for the shell to exit
    child: Arc<Mutex<ChildBox>>,
}

impl Drop for Term {
    fn drop(&mut self) {
        let _ = crate::lock(&self.child).kill();
    }
}

#[derive(Default)]
pub struct Terminals {
    terms: Mutex<HashMap<u32, Term>>,
    next: AtomicU32,
}

#[derive(Clone, Serialize)]
struct Data {
    id: u32,
    data: String,
}

#[derive(Clone, Serialize)]
struct Exit {
    id: u32,
}

fn err(e: impl std::fmt::Display) -> String {
    format!("{e:#}")
}

// the user's shell
fn shell() -> CommandBuilder {
    if cfg!(windows) {
        // PowerShell 7 when it's installed, else Windows PowerShell
        let pwsh = canopy_texkit::find_program(
            "pwsh",
            &canopy_texkit::search_path(&[]),
            &canopy_texkit::exe_extensions(),
        );
        let mut c = CommandBuilder::new(
            pwsh.map(|p| p.into_os_string())
                .unwrap_or_else(|| "powershell.exe".into()),
        );
        c.arg("-NoLogo");
        c
    } else {
        let sh = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
        let mut c = CommandBuilder::new(sh);
        c.arg("-l");
        c
    }
}

// the longest prefix of `buf` that is whole UTF-8; the rest waits for the
// next read, so a character split across reads isn't garbled
fn split_utf8(buf: &[u8]) -> usize {
    match std::str::from_utf8(buf) {
        Ok(_) => buf.len(),
        Err(e) if e.error_len().is_none() => e.valid_up_to(),
        Err(_) => buf.len(),
    }
}

/// Opens a terminal in a synced project's folder. Returns its id.
#[tauri::command]
pub fn term_open(app: AppHandle, link_id: String, cols: u16, rows: u16) -> Result<u32, String> {
    let state = app.state::<AppState>();
    let folder = {
        let cfg = state.config.read().unwrap_or_else(|e| e.into_inner());
        cfg.links
            .iter()
            .find(|l| l.id == link_id)
            .map(|l| l.folder.clone())
            .ok_or("that project isn't synced to a folder")?
    };
    if crate::lock(&state.terminals.terms).len() >= MAX_TERMINALS {
        return Err("too many terminals open; close one first".into());
    }
    let pty = native_pty_system()
        .openpty(PtySize {
            rows: rows.clamp(2, 500),
            cols: cols.clamp(2, 1000),
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(err)?;
    let mut cmd = shell();
    cmd.cwd(&folder);
    cmd.env("TERM", "xterm-256color");
    let child = pty.slave.spawn_command(cmd).map_err(err)?;
    drop(pty.slave);
    let mut reader = pty.master.try_clone_reader().map_err(err)?;
    let writer = pty.master.take_writer().map_err(err)?;
    let id = state.terminals.next.fetch_add(1, Ordering::Relaxed) + 1;

    let child: Arc<Mutex<ChildBox>> = Arc::new(Mutex::new(child));
    // the shell ending is told once, by whichever notices first: the reader
    // at end of output, or the waiter (on Windows the pty's output pipe can
    // stay open after the shell exits, so EOF alone isn't enough)
    let ended = Arc::new(AtomicBool::new(false));
    let finish = {
        let app = app.clone();
        let ended = ended.clone();
        move || {
            if !ended.swap(true, Ordering::SeqCst) {
                let _ = app.emit_to("main", "term-exit", Exit { id });
                // dropped on its own thread: dropping kills and waits
                let term = crate::lock(&app.state::<AppState>().terminals.terms).remove(&id);
                drop(term);
            }
        }
    };
    {
        let (child, finish, ended) = (child.clone(), finish.clone(), ended.clone());
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_millis(300));
            if ended.load(Ordering::SeqCst) {
                return;
            }
            let exited = !matches!(crate::lock(&child).try_wait(), Ok(None));
            if exited {
                // let the reader pass on the last output first
                std::thread::sleep(Duration::from_millis(200));
                finish();
                return;
            }
        });
    }

    let app2 = app.clone();
    std::thread::spawn(move || {
        let mut buf = vec![0u8; 16 * 1024];
        let mut carry: Vec<u8> = Vec::new();
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    carry.extend_from_slice(&buf[..n]);
                    let cut = split_utf8(&carry);
                    let text = String::from_utf8_lossy(&carry[..cut]).into_owned();
                    carry.drain(..cut);
                    let _ = app2.emit_to("main", "term-data", Data { id, data: text });
                }
            }
        }
        let _ = &app2;
        finish();
    });

    crate::lock(&state.terminals.terms).insert(
        id,
        Term {
            master: pty.master,
            writer,
            child,
        },
    );
    Ok(id)
}

#[tauri::command]
pub fn term_write(app: AppHandle, id: u32, data: String) -> Result<(), String> {
    let state = app.state::<AppState>();
    let mut terms = crate::lock(&state.terminals.terms);
    let t = terms.get_mut(&id).ok_or("that terminal has closed")?;
    t.writer.write_all(data.as_bytes()).map_err(err)?;
    t.writer.flush().map_err(err)
}

#[tauri::command]
pub fn term_resize(app: AppHandle, id: u32, cols: u16, rows: u16) -> Result<(), String> {
    let state = app.state::<AppState>();
    let terms = crate::lock(&state.terminals.terms);
    let t = terms.get(&id).ok_or("that terminal has closed")?;
    t.master
        .resize(PtySize {
            rows: rows.clamp(2, 500),
            cols: cols.clamp(2, 1000),
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(err)
}

// async, and the drop on a blocking thread: killing a shell can take a while,
// and must not hold up the window
#[tauri::command]
pub async fn term_close(app: AppHandle, id: u32) {
    let term = crate::lock(&app.state::<AppState>().terminals.terms).remove(&id);
    let _ = tauri::async_runtime::spawn_blocking(move || drop(term)).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_split_keeps_partial_characters() {
        let s = "aé".as_bytes(); // 'é' is two bytes
        assert_eq!(split_utf8(s), 3);
        assert_eq!(split_utf8(&s[..2]), 1);
        assert_eq!(split_utf8(b"\xff\xfe"), 2);
    }
}
