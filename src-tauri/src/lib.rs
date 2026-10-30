// lib.rs
// the tauri app: plugins, state, window/tray behaviour and the commands the
// svelte frontend calls. the real work lives in the crates/ workspace members.

mod accounts;
pub mod config;
#[cfg(feature = "full")]
mod dashboard;
#[cfg(feature = "full")]
mod local;
#[cfg(feature = "full")]
mod lsp;
#[cfg(feature = "full")]
mod search;
#[cfg(feature = "full")]
mod terminal;
#[cfg(feature = "full")]
mod tools;
#[cfg(feature = "full")]
mod zotero;
#[macro_use]
pub mod log_util;
// kept identical to the copy every Londopy app ships. tauri hands files to the
// running copy through single-instance, so the inbox half goes unused here
#[allow(dead_code)]
pub mod londopy;
mod settings_file;
#[cfg(feature = "full")]
mod syncing;
mod tray;
mod updater;
mod web;

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
    // web mode's pages; see web::WebCtl for why it's an async lock
    web: tauri::async_runtime::Mutex<web::WebCtl>,
    #[cfg(feature = "full")]
    syncer: syncing::Syncer,
    #[cfg(feature = "full")]
    local: local::Local,
    #[cfg(feature = "full")]
    lsp: lsp::Lsp,
    #[cfg(feature = "full")]
    terminals: terminal::Terminals,
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

// saves everything but the accounts and sync links, which only change through
// their own commands, so a settings save from a stale copy can't undo an add
// or remove
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
    change_config(&state, |c| {
        *c = AppConfig {
            accounts: std::mem::take(&mut c.accounts),
            links: std::mem::take(&mut c.links),
            ..cfg
        };
        Ok(())
    })?;
    // lite mode switches the folder watchers off (and back on)
    #[cfg(feature = "full")]
    syncing::refresh_watchers(&app);
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

// applies a change to the config and saves; the config only changes in
// memory once it's on disk
pub(crate) fn change_config<T>(
    state: &AppState,
    f: impl FnOnce(&mut AppConfig) -> anyhow::Result<T>,
) -> Result<(AppConfig, T), String> {
    let mut cfg = state.config.write().unwrap_or_else(|e| e.into_inner());
    let mut next = cfg.clone();
    let out = f(&mut next).map_err(err_any)?;
    next.save(&config::config_path()).map_err(err_any)?;
    *cfg = next.clone();
    Ok((next, out))
}

#[tauri::command]
fn add_account(
    state: State<'_, AppState>,
    label: String,
    server: String,
) -> Result<AppConfig, String> {
    let (cfg, account) = change_config(&state, |c| accounts::add(c, &label, &server))?;
    dlog!("added account {} for {}", account.id, account.server);
    Ok(cfg)
}

#[tauri::command]
fn rename_account(
    state: State<'_, AppState>,
    id: String,
    label: String,
) -> Result<AppConfig, String> {
    Ok(change_config(&state, |c| accounts::rename(c, &id, &label))?.0)
}

// removes the account and deletes its browser profile, which signs it out
#[tauri::command]
async fn remove_account(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<AppConfig, String> {
    if !accounts::valid_id(&id) {
        return Err("bad account id".into());
    }
    // its synced projects stop syncing; their folders stay
    #[cfg(feature = "full")]
    syncing::drop_account_links(&app, &id);
    // after any show/hide still in flight, so the page can't reappear
    let mut web = state.web.lock().await;
    let (cfg, _) = change_config(&state, |c| accounts::remove(c, &id))?;
    web::forget(&app, &mut web, &id).await;
    Ok(cfg)
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
        // open_js_links_on_click would also hijack target=_blank links inside
        // the Overleaf pages and send them to a command those pages can't
        // call, so they'd go nowhere; web.rs routes new windows itself
        .plugin(
            tauri_plugin_opener::Builder::new()
                .open_js_links_on_click(false)
                .build(),
        )
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(AppState {
            config: RwLock::new(cfg),
            tray_update: Mutex::new(None),
            startup_file: Mutex::new(
                londopy_arg(&args).map(|p| std::path::absolute(&p).unwrap_or(p)),
            ),
            tray_ok: AtomicBool::new(false),
            web: Default::default(),
            #[cfg(feature = "full")]
            syncer: Default::default(),
            #[cfg(feature = "full")]
            local: Default::default(),
            #[cfg(feature = "full")]
            lsp: Default::default(),
            #[cfg(feature = "full")]
            terminals: Default::default(),
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

            // profiles of removed accounts that a webview held on to last time
            let known: Vec<String> = app
                .state::<AppState>()
                .config
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .accounts
                .iter()
                .map(|a| a.id.clone())
                .collect();
            std::thread::spawn(move || web::sweep(&known));

            // folder watchers and the periodic pull for synced projects
            #[cfg(feature = "full")]
            syncing::start(&handle);

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
        .invoke_handler(handler())
        .build(tauri::generate_context!())
        .expect("error while starting Canopy")
        .run(|_app, _event| {
            // quitting stops any compile still running, so no TeX program
            // outlives Canopy
            #[cfg(feature = "full")]
            if let tauri::RunEvent::Exit = _event {
                local::cancel_all(_app);
            }
        });
}

// every build's commands, plus whatever the build adds
macro_rules! commands {
    ($($extra:path),* $(,)?) => {
        tauri::generate_handler![
            app_info,
            get_config,
            set_config,
            detect_tex,
            export_settings,
            read_settings_file,
            open_data_dir,
            route_londopy,
            take_startup_file,
            add_account,
            rename_account,
            remove_account,
            web::web_show,
            web::web_bounds,
            web::web_hide,
            web::web_action,
            updater::check_update,
            updater::install_update,
            $($extra),*
        ]
    };
}

#[cfg(feature = "full")]
fn handler() -> impl Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool + Send + Sync + 'static {
    commands![
        syncing::list_projects,
        syncing::link_project,
        syncing::unlink_project,
        syncing::set_link_auto,
        syncing::sync_now,
        syncing::sync_statuses,
        syncing::link_history,
        syncing::snapshot_changes,
        syncing::snapshot_diff,
        syncing::restore_snapshot,
        syncing::open_link_folder,
        local::local_files,
        local::read_text,
        local::write_text,
        local::create_text,
        local::main_file,
        local::set_main_file,
        local::compile_project,
        local::cancel_compile,
        local::read_pdf,
        local::synctex_forward,
        local::synctex_backward,
        search::search_project,
        terminal::term_open,
        terminal::term_write,
        terminal::term_resize,
        terminal::term_close,
        lsp::lsp_start,
        lsp::lsp_request,
        lsp::lsp_notify,
        lsp::lsp_stop,
        lsp::install_texlab,
        zotero::zotero_status,
        zotero::zotero_search,
        zotero::zotero_add,
        dashboard::dashboard,
        dashboard::set_goals,
    ]
}

#[cfg(not(feature = "full"))]
fn handler() -> impl Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool + Send + Sync + 'static {
    commands![]
}
