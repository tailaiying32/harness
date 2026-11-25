//! Auto update. The app looks for a newer release on GitHub (the `latest.json`
//! that `scripts/mac-release.sh` uploads), downloads it in the background, and
//! keeps it until "update" in the sidebar puts it in place and restarts. The
//! download is checked against the update key in `tauri.conf.json`, so only a
//! release signed with its private half is installed.
//!
//! Nothing is installed without that click: quitting leaves the app as it was,
//! and the next start downloads the update again.

use tauri::{AppHandle, State};
use tauri_plugin_updater::{Update, UpdaterExt};
use tokio::sync::Mutex;

/// The update downloaded and waiting for the click, if any. Held across a
/// whole check, so two checks at once download it only once.
#[derive(Default)]
pub struct Updates(Mutex<Option<(Update, Vec<u8>)>>);

/// Looks for a newer version and downloads it. Gives its version once it is
/// ready to install, or nothing when this one is the latest. Builds for working
/// on the app (`npm run tauri dev`) never look.
#[tauri::command]
pub async fn update_check(app: AppHandle, updates: State<'_, Updates>) -> Result<Option<String>, String> {
    if cfg!(debug_assertions) {
        return Ok(None);
    }
    let mut staged = updates.0.lock().await;
    if let Some((update, _)) = staged.as_ref() {
        return Ok(Some(update.version.clone()));
    }
    let found = app.updater().map_err(|e| e.to_string())?.check().await.map_err(|e| e.to_string())?;
    let Some(update) = found else { return Ok(None) };
    let bytes = update.download(|_, _| {}, || {}).await.map_err(|e| e.to_string())?;
    let version = update.version.clone();
    *staged = Some((update, bytes));
    Ok(Some(version))
}

/// Puts the downloaded update in place of the running app, then restarts into
/// it. Quitting runs as usual first, so chats finish up as they do on any quit.
/// When the app sits in a folder the user can't write to, the Mac asks for an
/// admin password; a refusal keeps the update for another try.
#[tauri::command]
pub async fn update_install(app: AppHandle, updates: State<'_, Updates>) -> Result<(), String> {
    let mut staged = updates.0.lock().await;
    let Some((update, bytes)) = staged.take() else {
        return Err("No update is ready.".into());
    };
    // Off this thread: the password prompt runs on the main one.
    let (update, bytes, installed) = tauri::async_runtime::spawn_blocking(move || {
        let installed = update.install(&bytes);
        (update, bytes, installed)
    })
    .await
    .map_err(|e| e.to_string())?;
    if let Err(e) = installed {
        *staged = Some((update, bytes));
        return Err(e.to_string());
    }
    app.request_restart();
    Ok(())
}
