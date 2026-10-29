// lib.rs
// the tauri app: plugins, state, window/tray behaviour and the commands the
// svelte frontend calls. the real work lives in the crates/ workspace members.

pub mod config;
#[macro_use]
pub mod log_util;
// kept identical to the copy every Londopy app ships. tauri hands files to the
// running copy through single-instance, so the inbox half goes unused here
#[allow(dead_code)]
pub mod londopy;
mod settings_file;
mod tray;
mod updater;

use config::AppConfig;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, RwLock};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
use tauri_plugin_autostart::ManagerExt;

pub struct AppState {
    config: RwLock<AppConfig>,
    tray_update: Mutex<Option<String>>,
    startup_file: Mutex<Option<PathBuf>>,
    tray_ok: AtomicBool,
}

// a poisoned lock only means another thread panicked mid-update; the data is
// still the best we have
pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

pub(crate) fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn err(e: impl std::fmt::Display) -> String {
    format!("{e:#}")
}

fn err_any(e: anyhow::Error) -> String {
    format!("{e:#}")
}

// ---------------------------------------------------------------- commands

#[derive(Serialize)]
struct AppInfo {
    name: &'static str,
    version: &'static str,
    build_date: &'static str,
    // false for Canopy Lite (built without the `full` feature)
    full_build: bool,
    portable: bool,
    // true when an installer (nsis, msi, dmg, AppImage, deb, rpm) put this
    // copy here, so the updater can replace it. a portable exe or a dev build
    // gets a link to the releases page instead
    can_self_update: bool,
    data_dir: PathBuf,
    os: &'static str,
    arch: &'static str,
    homepage: &'static str,
}

#[tauri::command]
fn app_info() -> AppInfo {
    AppInfo {
        name: "Canopy",
        version: env!("CARGO_PKG_VERSION"),
        build_date: env!("BUILD_DATE"),
        full_build: cfg!(feature = "full"),
        portable: config::is_portable(),
        can_self_update: can_self_update(),
        data_dir: config::data_dir(),
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        homepage: londopy::HOMEPAGE,
    }
}

#[tauri::command]
fn get_config(state: State<'_, AppState>) -> AppConfig {
    state
        .config
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

// the bundler stamps the bundle type into the binary; the bare exe copied out
// as the portable build, and dev builds, have none
fn can_self_update() -> bool {
    !config::is_portable() && tauri::utils::platform::bundle_type().is_some()
}

#[tauri::command]
fn set_config(app: AppHandle, state: State<'_, AppState>, cfg: AppConfig) -> Result<(), String> {
    let was_on = state
        .config
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .general
        .start_with_system;
    // autostart first: if the OS refuses, the setting must not claim it's on
    if was_on != cfg.general.start_with_system {
        apply_autostart(&app, cfg.general.start_with_system).map_err(err)?;
    }
    cfg.save(&config::config_path()).map_err(err_any)?;
    *state.config.write().unwrap_or_else(|e| e.into_inner()) = cfg;
    Ok(())
}

fn apply_autostart(app: &AppHandle, on: bool) -> Result<(), tauri_plugin_autostart::Error> {
    let al = app.autolaunch();
    if on {
        al.enable()
    } else if al.is_enabled().unwrap_or(false) {
        al.disable()
    } else {
        Ok(())
    }
}

// files from the frontend: only .londopy, so these commands can't be pointed
// at anything else on disk
fn require_londopy(path: &std::path::Path) -> Result<(), String> {
    if londopy::is_londopy_path(path) {
        Ok(())
    } else {
        Err(format!("{} is not a .londopy file", path.display()))
    }
}

#[derive(Serialize)]
struct TexReport {
    tools: canopy_texkit::Toolchain,
    // what a compile would use right now under the configured preference
    engine: Option<canopy_texkit::Engine>,
}

#[tauri::command]
async fn detect_tex(state: State<'_, AppState>) -> Result<TexReport, String> {
    let pref = state
        .config
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .compile
        .engine;
    // tools Canopy downloads itself go here, ahead of PATH
    let own = vec![config::data_dir().join("tools")];
    let tools =
        tauri::async_runtime::spawn_blocking(move || canopy_texkit::Toolchain::detect(&own))
            .await
            .map_err(err)?;
    let engine = tools.choose(pref);
    Ok(TexReport { tools, engine })
}

#[tauri::command]
fn export_settings(state: State<'_, AppState>, path: PathBuf) -> Result<(), String> {
    require_londopy(&path)?;
    let cfg = state
        .config
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    settings_file::export(&path, &cfg).map_err(err_any)
}

// reads a settings .londopy and returns it; the frontend shows it and saves it
// through set_config once the user confirms
#[tauri::command]
fn read_settings_file(state: State<'_, AppState>, path: PathBuf) -> Result<AppConfig, String> {
    require_londopy(&path)?;
    let current = state
        .config
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    settings_file::import(&path, &current).map_err(err_any)
}

#[tauri::command]
fn open_data_dir() -> Result<(), String> {
    let dir = config::data_dir();
    std::fs::create_dir_all(&dir).map_err(err)?;
    tauri_plugin_opener::open_path(dir, None::<&str>).map_err(err)
}

// ---------------------------------------------------------------- startup

// a .londopy passed on the command line (double-click, or dropped on the exe)
fn londopy_arg(args: &[String]) -> Option<PathBuf> {
    match londopy::parse_args(args)? {
        londopy::LondopyArg::Open(p) => Some(p),
        londopy::LondopyArg::Unregister => None,
    }
}

#[derive(Serialize)]
#[serde(tag = "result", rename_all = "kebab-case")]
enum Routed {
    // ours: the frontend opens it
    Mine { kind: String, path: PathBuf },
    // handed to the Londopy app it belongs to
    Forwarded { app: String },
    // belongs to an app that isn't installed
    NotInstalled { app: String, homepage: String },
}

// the frontend calls this for every .londopy it is handed, from the command
// line at startup or from a second launch
#[tauri::command]
fn route_londopy(path: PathBuf) -> Result<Routed, String> {
    require_londopy(&path)?;
    Ok(match londopy::dispatch(&path).map_err(err_any)? {
        londopy::Dispatch::Mine(m) => Routed::Mine { kind: m.kind, path },
        londopy::Dispatch::Forwarded { app } => Routed::Forwarded { app },
        londopy::Dispatch::NotInstalled { app, homepage } => Routed::NotInstalled { app, homepage },
    })
}

// the file Canopy was started with, handed out once. pulling it (instead of
// emitting at startup) means the frontend can't miss it while still loading
#[tauri::command]
fn take_startup_file(state: State<'_, AppState>) -> Option<PathBuf> {
    lock(&state.startup_file).take()
}

pub fn run() {
    let args: Vec<String> = std::env::args().collect();
    // the uninstaller runs us with this; do it and leave before any window exists
    if let Some(londopy::LondopyArg::Unregister) = londopy::parse_args(&args) {
        let _ = londopy::unregister_self();
        return;
    }

    std::panic::set_hook(Box::new(|info| {
        dlog!("panic: {info}");
    }));

    let cfg = AppConfig::load_or_default(&config::config_path());
    dlog!(
        "starting Canopy {} ({})",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS
    );

    tauri::Builder::default()
        // first: a second launch only surfaces this one (and hands over its file)
        .plugin(tauri_plugin_single_instance::init(|app, argv, cwd| {
            show_main(app);
            if let Some(p) = londopy_arg(&argv) {
                // the second launch may have run from another folder
                let p = if p.is_relative() {
                    PathBuf::from(cwd).join(p)
                } else {
                    p
                };
                let _ = app.emit("londopy-path", p);
            }
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--hidden"]),
        ))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(AppState {
            config: RwLock::new(cfg),
            tray_update: Mutex::new(None),
            startup_file: Mutex::new(
                londopy_arg(&args).map(|p| std::path::absolute(&p).unwrap_or(p)),
            ),
            tray_ok: AtomicBool::new(false),
        })
        .setup(move |app| {
            let handle = app.handle().clone();
            // no tray (GNOME without AppIndicator, say) isn't fatal, but then
            // closing the window has to quit, or it could never come back
            match tray::build(&handle) {
                Ok(()) => app
                    .state::<AppState>()
                    .tray_ok
                    .store(true, Ordering::Relaxed),
                Err(e) => dlog!("no tray icon: {e}"),
            }

            if !config::is_portable() {
                if let Err(e) = londopy::register_self() {
                    dlog!("londopy register failed: {e:#}");
                }
            }

            // an update or reinstall can move the exe; re-enabling rewrites the path
            let al = app.autolaunch();
            if al.is_enabled().unwrap_or(false) {
                let _ = al.enable();
            }

            if args.iter().any(|a| a == "--hidden") {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.hide();
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let state = window.state::<AppState>();
                let to_tray = state.tray_ok.load(Ordering::Relaxed)
                    && state
                        .config
                        .read()
                        .map(|c| c.general.close_to_tray)
                        .unwrap_or(true);
                if to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            get_config,
            set_config,
            detect_tex,
            export_settings,
            read_settings_file,
            open_data_dir,
            route_londopy,
            take_startup_file,
            updater::check_update,
            updater::install_update,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Canopy");
}
