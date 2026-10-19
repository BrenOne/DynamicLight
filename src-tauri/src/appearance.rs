//! Installation-local appearance, shared by studio and Quick controls.
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Mutex};
use tauri::{Emitter, Manager};

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Accent {
    #[default]
    Violet,
    Blue,
    Green,
    Rose,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Density {
    Compact,
    #[default]
    Spacious,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Preferences {
    theme: Theme,
    accent: Accent,
    density: Density,
}
struct Store {
    value: Mutex<Preferences>,
    path: PathBuf,
}
fn apply(app: &tauri::AppHandle, value: &Preferences) {
    app.set_theme(match value.theme {
        Theme::System => None,
        Theme::Light => Some(tauri::Theme::Light),
        Theme::Dark => Some(tauri::Theme::Dark),
    });
}
pub fn init(app: &tauri::AppHandle) -> Result<(), String> {
    let path = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("appearance.json");
    let value = std::fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    apply(app, &value);
    app.manage(Store {
        value: Mutex::new(value),
        path,
    });
    Ok(())
}
#[tauri::command]
pub fn appearance(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    value: Option<Preferences>,
) -> Result<Preferences, String> {
    let store = app.state::<Store>();
    let mut current = store.value.lock().map_err(|e| e.to_string())?;
    if let Some(value) = value {
        if window.label() != "main" {
            return Err("Change appearance in the studio".into());
        }
        crate::settings::save_json(&value, &store.path)?;
        apply(&app, &value);
        *current = value;
        let _ = app.emit("appearance-changed", &*current);
    }
    Ok(current.clone())
}
