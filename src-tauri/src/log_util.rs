// log_util.rs
// a tiny file logger: timestamped lines in <data dir>/debug.log. never fails
// the caller, and rotates past ~512 KB keeping one old file (debug.log.1)

use std::io::Write;
use std::sync::Mutex;

const MAX_BYTES: u64 = 512 * 1024;

static LOCK: Mutex<()> = Mutex::new(());

pub fn write(msg: &str) {
    if cfg!(debug_assertions) {
        eprintln!("{msg}");
    }
    // unit tests shouldn't write into the real data dir
    if cfg!(test) {
        return;
    }
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = crate::config::data_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join("debug.log");
    if std::fs::metadata(&path)
        .map(|m| m.len() > MAX_BYTES)
        .unwrap_or(false)
    {
        let _ = std::fs::rename(&path, dir.join("debug.log.1"));
    }
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let stamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
        let _ = writeln!(f, "[{stamp}] {msg}");
    }
}

// logging that reads like eprintln!
#[macro_export]
macro_rules! dlog {
    ($($arg:tt)*) => { $crate::log_util::write(&format!($($arg)*)) };
}
