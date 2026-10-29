// tray.rs
// the tray icon: a status line, open, the update item while one is waiting,
// quit. left click brings the window back.

use crate::AppState;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};

pub const TRAY_ID: &str = "canopy-tray";

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let menu = menu(app, None)?;
    let mut tray = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("Canopy")
        .on_menu_event(|app, e| on_menu(app, e.id.as_ref()))
        .on_tray_icon_event(|tray, ev| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = ev
            {
                crate::show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon().cloned() {
        tray = tray.icon(icon);
    }
    tray.build(app)?;
    Ok(())
}

fn menu(app: &AppHandle, update: Option<&str>) -> tauri::Result<Menu<tauri::Wry>> {
    let menu = Menu::new(app)?;
    let status = format!("Canopy v{}", env!("CARGO_PKG_VERSION"));
    menu.append(&MenuItem::with_id(
        app,
        "status",
        status,
        false,
        None::<&str>,
    )?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(
        app,
        "show",
        "Open Canopy",
        true,
        None::<&str>,
    )?)?;
    if let Some(v) = update {
        let label = format!("Update to v{v}…");
        menu.append(&MenuItem::with_id(
            app,
            "update",
            label,
            true,
            None::<&str>,
        )?)?;
    }
    menu.append(&MenuItem::with_id(
        app,
        "settings",
        "Settings…",
        true,
        None::<&str>,
    )?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(
        app,
        "quit",
        "Quit Canopy",
        true,
        None::<&str>,
    )?)?;
    Ok(menu)
}

// called when the frontend finds (or clears) an update
pub fn set_update(app: &AppHandle, version: Option<String>) {
    *crate::lock(&app.state::<AppState>().tray_update) = version;
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let v = crate::lock(&handle.state::<AppState>().tray_update).clone();
        if let (Some(tray), Ok(m)) = (handle.tray_by_id(TRAY_ID), menu(&handle, v.as_deref())) {
            let _ = tray.set_menu(Some(m));
        }
    });
}

fn on_menu(app: &AppHandle, id: &str) {
    match id {
        "show" => crate::show_main(app),
        "quit" => app.exit(0),
        // the frontend owns the banner and the settings view; just point it there
        "update" | "settings" => {
            crate::show_main(app);
            let _ = app.emit(&format!("tray-{id}"), ());
        }
        _ => {}
    }
}
