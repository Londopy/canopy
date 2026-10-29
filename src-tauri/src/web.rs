// web.rs
// web mode: the real Overleaf site in a webview embedded in the main window,
// one per account. the svelte shell draws an empty box where the page goes
// and tells us its position; we lay the native webview over it.
//
// each account gets its own browser profile (data folder on windows/linux,
// data store on macos 14+), so its cookies, and so its sign-in, never mix
// with another account's. canopy itself doesn't read the session yet; you
// sign in on overleaf's own page, in that profile.
//
// these webviews load remote pages. the capability in capabilities/default.json
// names only the shell's own webview ("main"), and on_navigation below keeps
// them off canopy's own origin, so an overleaf page can't call canopy's commands.

use crate::{accounts, config, AppState};
use canopy_overleaf::Server;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::webview::{NewWindowResponse, PageLoadEvent, WebviewBuilder};
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, State, Url, WebviewUrl};

const LABEL_PREFIX: &str = "acct-";

/// Which account's page is showing. Every command that creates, shows, hides,
/// moves or closes a page takes this lock first, so they run one at a time in
/// the order they were called: creating a webview takes a moment, and a hide
/// that overtook it would leave the page painted over the palette.
#[derive(Default)]
pub struct WebCtl {
    active: Option<String>,
}

fn err(e: impl std::fmt::Display) -> String {
    format!("{e:#}")
}

fn label(id: &str) -> String {
    format!("{LABEL_PREFIX}{id}")
}

fn profiles_root() -> PathBuf {
    config::data_dir().join("webviews")
}

// where an account's cookies, local storage and cache live
pub fn profile_dir(id: &str) -> PathBuf {
    profiles_root().join(id)
}

// macos keys data stores by a 16-byte id rather than a folder; derive a
// stable one from the account id
fn store_id(id: &str) -> [u8; 16] {
    let mut out = [0u8; 16];
    // fnv-1a, twice with different seeds, is plenty to keep a handful of ids apart
    for (half, seed) in [0xcbf2_9ce4_8422_2325u64, 0x8422_2325_cbf2_9ce4]
        .iter()
        .enumerate()
    {
        let mut h = *seed;
        for b in id.bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        out[half * 8..half * 8 + 8].copy_from_slice(&h.to_le_bytes());
    }
    out
}

/// The box in the main window where the page goes, in CSS (logical) pixels.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct Rect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

/// What the shell shows above the page: where it is and whether it's signed in.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageState {
    id: String,
    url: String,
    // None while on a page that doesn't tell us (an SSO provider's site, a
    // public page like the homepage)
    signed_in: Option<bool>,
    loading: bool,
}

// overleaf's pages say whether you're signed in: the sign-in and registration
// pages mean no, the project pages (which redirect to sign-in otherwise) mean
// yes, and anything else (the homepage, help, blog) doesn't say
pub fn signed_in_from(server: &Server, url: &Url) -> Option<bool> {
    if !same_origin(server, url) {
        return None;
    }
    let path = url.path();
    let under = |p: &str| path == p || path.starts_with(&format!("{p}/"));
    if ["/login", "/register", "/user/password", "/saml", "/oauth"]
        .iter()
        .any(|p| under(p))
    {
        Some(false)
    } else if under("/project") {
        Some(true)
    } else {
        None
    }
}

// scheme, host and port all match the account's server. Url lowercases the
// host, drops default ports and keeps IPv6 brackets, the same as Server does
pub fn same_origin(server: &Server, url: &Url) -> bool {
    let origin = match (url.host_str(), url.port()) {
        (Some(h), Some(p)) => format!("{}://{h}:{p}", url.scheme()),
        (Some(h), None) => format!("{}://{h}", url.scheme()),
        _ => return false,
    };
    origin == server.base_url()
}

// an account's page may go anywhere on the web (SSO providers included), but
// never to canopy's own origin, where it would count as the app itself
pub fn navigation_allowed(url: &Url) -> bool {
    match url.scheme() {
        "http" | "https" => !matches!(
            url.host_str(),
            Some("tauri.localhost" | "ipc.localhost" | "asset.localhost")
        ),
        "about" | "blob" | "data" => true,
        _ => false,
    }
}

fn emit_state(app: &AppHandle, state: PageState) {
    let _ = app.emit("web-state", state);
}

fn create(
    app: &AppHandle,
    account: &config::Account,
    rect: Rect,
) -> Result<tauri::Webview, String> {
    let window = app.get_window("main").ok_or("the main window is gone")?;
    let start: Url = account.server.projects_url().parse().map_err(err)?;
    let id = account.id.clone();
    let server = account.server.clone();

    let (nav_app, nav_id, nav_server) = (app.clone(), id.clone(), server.clone());
    let (win_server, title_app, title_id) = (server.clone(), app.clone(), id.clone());

    let builder = WebviewBuilder::new(label(&id), WebviewUrl::External(start))
        .data_directory(profile_dir(&id))
        .data_store_identifier(store_id(&id))
        // tauri's own drop handler swallows HTML5 drag and drop on windows;
        // overleaf needs it for uploads and moving files in its tree
        .disable_drag_drop_handler()
        .on_navigation(|url| {
            let ok = navigation_allowed(url);
            if !ok {
                crate::dlog!("blocked a page from navigating to {url}");
            }
            ok
        })
        .on_page_load(move |_wv, payload| {
            let url = payload.url().clone();
            emit_state(
                &nav_app,
                PageState {
                    id: nav_id.clone(),
                    signed_in: signed_in_from(&nav_server, &url),
                    url: url.to_string(),
                    loading: matches!(payload.event(), PageLoadEvent::Started),
                },
            );
        })
        .on_document_title_changed(move |_wv, title| {
            let _ = title_app.emit("web-title", (title_id.clone(), title));
        })
        // popups on the account's own server (a PDF, the history view, a
        // shared link) open in a window that shares its profile; anything
        // else goes to the system browser, where links normally go
        .on_new_window(move |url, _features| {
            if same_origin(&win_server, &url) {
                NewWindowResponse::Allow
            } else {
                if matches!(url.scheme(), "http" | "https" | "mailto") {
                    let _ = tauri_plugin_opener::open_url(url.as_str(), None::<&str>);
                }
                NewWindowResponse::Deny
            }
        });

    window
        .add_child(
            builder,
            LogicalPosition::new(rect.x, rect.y),
            LogicalSize::new(rect.width.max(1.0), rect.height.max(1.0)),
        )
        .map_err(err)
}

fn account_webviews(app: &AppHandle) -> impl Iterator<Item = tauri::Webview> {
    app.webviews()
        .into_values()
        .filter(|w| w.label().starts_with(LABEL_PREFIX))
}

fn place(wv: &tauri::Webview, rect: Rect) -> Result<(), String> {
    wv.set_position(LogicalPosition::new(rect.x, rect.y))
        .map_err(err)?;
    wv.set_size(LogicalSize::new(rect.width.max(1.0), rect.height.max(1.0)))
        .map_err(err)
}

/// Shows the account's page in `rect`, creating its webview the first time,
/// and hides every other account's.
// async so it runs off the main thread: add_child hands the build to the main
// thread and waits for it
#[tauri::command]
pub async fn web_show(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    rect: Rect,
) -> Result<(), String> {
    let mut ctl = state.web.lock().await;
    let account = {
        let cfg = state.config.read().unwrap_or_else(|e| e.into_inner());
        accounts::find(&cfg, &id).map_err(err)?.clone()
    };
    let wv = match app.get_webview(&label(&id)) {
        Some(wv) => wv,
        None => {
            crate::dlog!("opening the page for account {id}");
            create(&app, &account, rect).inspect_err(|e| {
                crate::dlog!("couldn't create the page for {id}: {e}");
            })?
        }
    };
    for other in account_webviews(&app).filter(|w| w.label() != wv.label()) {
        let _ = other.hide();
    }
    place(&wv, rect)?;
    wv.show().map_err(err)?;
    ctl.active = Some(id);
    Ok(())
}

/// Moves the visible page when the shell's layout changes (window resize,
/// sidebar toggle).
#[tauri::command]
pub async fn web_bounds(
    app: AppHandle,
    state: State<'_, AppState>,
    rect: Rect,
) -> Result<(), String> {
    let ctl = state.web.lock().await;
    if let Some(wv) = ctl
        .active
        .as_ref()
        .and_then(|id| app.get_webview(&label(id)))
    {
        place(&wv, rect)?;
    }
    Ok(())
}

/// Hides every page, for when the shell shows something else (another view,
/// the command palette): native webviews always draw on top of the shell.
#[tauri::command]
pub async fn web_hide(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let mut ctl = state.web.lock().await;
    for wv in account_webviews(&app) {
        let _ = wv.hide();
    }
    ctl.active = None;
    Ok(())
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Action {
    Back,
    Forward,
    Reload,
    Home,
    OpenInBrowser,
}

#[tauri::command]
pub fn web_action(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    action: Action,
) -> Result<(), String> {
    let wv = app
        .get_webview(&label(&id))
        .ok_or("that account's page isn't open")?;
    match action {
        Action::Back => wv.eval("history.back()").map_err(err),
        Action::Forward => wv.eval("history.forward()").map_err(err),
        Action::Reload => wv.reload().map_err(err),
        Action::Home => {
            let cfg = state.config.read().unwrap_or_else(|e| e.into_inner());
            let url = accounts::find(&cfg, &id)
                .map_err(err)?
                .server
                .projects_url();
            wv.navigate(url.parse().map_err(err)?).map_err(err)
        }
        Action::OpenInBrowser => {
            let url = wv.url().map_err(err)?;
            tauri_plugin_opener::open_url(url.as_str(), None::<&str>).map_err(err)
        }
    }
}

/// Closes an account's page and deletes its browser profile, which signs it
/// out. Call with the [`WebCtl`] lock held. The webview's helper processes can
/// hold files for a moment after it closes, so the folder delete retries in the
/// background; anything still left is swept at the next start ([`sweep`]).
pub async fn forget(app: &AppHandle, ctl: &mut WebCtl, id: &str) {
    if ctl.active.as_deref() == Some(id) {
        ctl.active = None;
    }
    if let Some(wv) = app.get_webview(&label(id)) {
        let _ = wv.close();
    }
    // macos keeps the profile in a data store, not the folder
    #[cfg(target_vendor = "apple")]
    if let Err(e) = app.remove_data_store(store_id(id)).await {
        crate::dlog!("couldn't remove the data store for {id}: {e}");
    }
    #[cfg(not(target_vendor = "apple"))]
    let _ = app;
    let dir = profile_dir(id);
    std::thread::spawn(move || {
        for _ in 0..20 {
            match std::fs::remove_dir_all(&dir) {
                Ok(()) => return,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
                Err(_) => std::thread::sleep(std::time::Duration::from_millis(500)),
            }
        }
        crate::dlog!("couldn't delete {} yet; the next start will", dir.display());
    });
}

/// At startup, deletes the profile folder of any account that's no longer in
/// the config (removed while a webview still held its files, or by hand).
pub fn sweep(known: &[String]) {
    let Ok(entries) = std::fs::read_dir(profiles_root()) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        // only folders this module made: an account id that isn't ours any more
        if accounts::valid_id(&name) && !known.contains(&name) && entry.path().is_dir() {
            match std::fs::remove_dir_all(entry.path()) {
                Ok(()) => crate::dlog!("deleted the leftover browser profile {name}"),
                Err(e) => crate::dlog!("couldn't delete the leftover profile {name}: {e}"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(s: &str) -> Url {
        s.parse().unwrap()
    }

    #[test]
    fn origin_matching() {
        let s = Server::parse("tex.example.org:8443").unwrap();
        assert!(same_origin(
            &s,
            &url("https://tex.example.org:8443/project")
        ));
        assert!(same_origin(&s, &url("https://TEX.example.org:8443/x")));
        assert!(!same_origin(&s, &url("https://tex.example.org/project")));
        assert!(!same_origin(&s, &url("http://tex.example.org:8443/")));
        assert!(!same_origin(
            &s,
            &url("https://tex.example.org.evil.com:8443/")
        ));

        let ol = Server::overleaf_com();
        assert!(same_origin(
            &ol,
            &url("https://www.overleaf.com:443/project")
        ));
        assert!(!same_origin(&ol, &url("https://overleaf.com/project")));

        let local = Server::parse("http://[::1]:8080").unwrap();
        assert!(same_origin(&local, &url("http://[::1]:8080/login")));
    }

    #[test]
    fn signed_in_from_the_page() {
        let s = Server::overleaf_com();
        let check = |u: &str| signed_in_from(&s, &url(u));
        assert_eq!(check("https://www.overleaf.com/project"), Some(true));
        assert_eq!(check("https://www.overleaf.com/project/abc"), Some(true));
        assert_eq!(check("https://www.overleaf.com/login"), Some(false));
        assert_eq!(
            check("https://www.overleaf.com/login?redir=/x"),
            Some(false)
        );
        assert_eq!(check("https://www.overleaf.com/register"), Some(false));
        // public pages don't say either way
        assert_eq!(check("https://www.overleaf.com/"), None);
        assert_eq!(check("https://www.overleaf.com/learn/latex"), None);
        assert_eq!(check("https://www.overleaf.com/loginhelp"), None);
        assert_eq!(check("https://www.overleaf.com/projects-old"), None);
        assert_eq!(check("https://accounts.google.com/o/oauth2"), None);
    }

    #[test]
    fn keeps_pages_off_the_app_origin() {
        assert!(navigation_allowed(&url("https://www.overleaf.com/project")));
        assert!(navigation_allowed(&url("http://localhost:8080/login")));
        assert!(navigation_allowed(&url("https://accounts.google.com/x")));
        assert!(navigation_allowed(&url("about:blank")));
        assert!(!navigation_allowed(&url("http://tauri.localhost/")));
        assert!(!navigation_allowed(&url("https://ipc.localhost/x")));
        assert!(!navigation_allowed(&url("tauri://localhost/")));
        assert!(!navigation_allowed(&url("file:///C:/Windows/win.ini")));
        assert!(!navigation_allowed(&url("javascript:alert(1)")));
    }

    #[test]
    fn store_ids_differ_and_are_stable() {
        assert_eq!(store_id("abc"), store_id("abc"));
        assert_ne!(store_id("abc"), store_id("abd"));
    }
}
