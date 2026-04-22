use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::Manager;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeInfo {
    app_name: String,
    app_version: String,
    app_data_dir: Option<String>,
    state_store_path: Option<String>,
    tauri_env: String,
    packaging_note: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NativeStateEnvelope {
    state_json: String,
}

#[tauri::command]
fn runtime_info(app: tauri::AppHandle) -> RuntimeInfo {
    let package_info = app.package_info();
    let app_data_dir = app
        .path()
        .app_data_dir()
        .ok()
        .map(|path| path.display().to_string());
    let state_store_path = state_file_path(&app)
        .ok()
        .map(|path| path.display().to_string());

    RuntimeInfo {
        app_name: package_info.name.clone(),
        app_version: package_info.version.to_string(),
        app_data_dir,
        state_store_path,
        tauri_env: if cfg!(debug_assertions) {
            "development".into()
        } else {
            "production".into()
        },
        packaging_note:
            "Native shell stores profile state in the app data directory and is ready for deeper SMTP/IMAP command wiring.".into(),
    }
}

#[tauri::command]
fn load_app_state(app: tauri::AppHandle) -> Result<Option<NativeStateEnvelope>, String> {
    let path = state_file_path(&app).map_err(|err| err.to_string())?;
    if !path.exists() {
        return Ok(None);
    }

    let state_json = fs::read_to_string(&path).map_err(|err| err.to_string())?;
    Ok(Some(NativeStateEnvelope { state_json }))
}

#[tauri::command]
fn save_app_state(app: tauri::AppHandle, state_json: String) -> Result<(), String> {
    let path = state_file_path(&app).map_err(|err| err.to_string())?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    fs::write(path, state_json).map_err(|err| err.to_string())
}

#[tauri::command]
fn reset_app_state(app: tauri::AppHandle) -> Result<(), String> {
    let path = state_file_path(&app).map_err(|err| err.to_string())?;
    if path.exists() {
        fs::remove_file(path).map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn state_file_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let base = app.path().app_data_dir().map_err(|err| err.to_string())?;
    Ok(base.join("state").join("app-state.json"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            runtime_info,
            load_app_state,
            save_app_state,
            reset_app_state
        ])
        .run(tauri::generate_context!())
        .expect("error while running Mimicrypt desktop");
}
