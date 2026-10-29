// updater.rs
// in-app updates through tauri-plugin-updater: it reads latest.json from the
// newest GitHub release and checks the minisign signature before installing.
// nothing installs until the user clicks "Install and restart".

use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_updater::UpdaterExt;

#[derive(Debug, Serialize)]
pub struct UpdateInfo {
    pub version: String,
    pub current: String,
    pub notes: Option<String>,
    pub date: Option<String>,
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[tauri::command]
pub async fn check_update(app: AppHandle) -> Result<Option<UpdateInfo>, String> {
    let updater = app.updater_builder().build().map_err(err)?;
    let found = updater.check().await.map_err(err)?;
    let info = found.map(|u| UpdateInfo {
        version: u.version.clone(),
        current: u.current_version.clone(),
        notes: u.body.clone(),
        date: u.date.map(|d| d.to_string()),
    });
    crate::tray::set_update(&app, info.as_ref().map(|i| i.version.clone()));
    Ok(info)
}

// downloads, verifies and installs, then restarts. on windows the nsis
// installer closes the app itself and starts it again afterwards
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    let updater = app.updater_builder().build().map_err(err)?;
    let Some(update) = updater.check().await.map_err(err)? else {
        return Err("No update available".into());
    };
    crate::dlog!("installing update {}", update.version);
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(err)?;
    app.restart()
}
