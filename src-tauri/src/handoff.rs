//! Portable scene + physical calibration. No USB path, capture consent or credentials.
use crate::{
    engine::{Action, Handle, Snapshot},
    settings::{SceneSettings, Settings},
};
use serde::{Deserialize, Serialize};
use tauri::Manager;
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Profile {
    format: String,
    version: u8,
    vendor: u16,
    product: u16,
    pub scene: SceneSettings,
    pub count: usize,
    pub sides: u8,
    pub offset: usize,
    pub reverse: bool,
}
impl Profile {
    pub fn capture(s: &Settings) -> Self {
        Self {
            format: "dynamiclight-handoff".into(),
            version: 1,
            vendor: crate::protocol::VID,
            product: crate::protocol::PID,
            scene: SceneSettings::capture(s),
            count: s.count,
            sides: s.sides,
            offset: s.offset,
            reverse: s.reverse,
        }
    }
    pub fn parse(text: &str) -> Result<Self, String> {
        if text.len() > 256 * 1024 {
            return Err("Handoff files must be at most 256 KiB".into());
        }
        let value: Self = serde_json::from_str(text).map_err(|e| e.to_string())?;
        value.validate()?;
        Ok(value)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.format != "dynamiclight-handoff"
            || self.version != 1
            || self.vendor != crate::protocol::VID
            || self.product != crate::protocol::PID
        {
            return Err("Unsupported handoff format or controller model".into());
        }
        self.scene.validate()?;
        Settings {
            count: self.count,
            sides: self.sides,
            offset: self.offset,
            ..Default::default()
        }
        .validate()
    }
}
#[tauri::command]
pub fn handoff_preview(text: String) -> Result<Profile, String> {
    Profile::parse(&text)
}
#[tauri::command]
pub async fn handoff_export(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
) -> Result<String, String> {
    if window.label() != "main" {
        return Err("Export handoff from the main window".into());
    }
    let name = format!(
        "DynamicLight-handoff-{}.json",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    );
    let path = app
        .path()
        .download_dir()
        .map_err(|e| e.to_string())?
        .join(name);
    let output = path.to_string_lossy().into_owned();
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Handle>()
            .request(Action::ExportHandoff { path })
    })
    .await
    .map_err(|e| e.to_string())??;
    Ok(output)
}
#[tauri::command]
pub async fn handoff_import(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    text: String,
) -> Result<Snapshot, String> {
    if window.label() != "main" {
        return Err("Import handoff from the main window".into());
    }
    let profile = Profile::parse(&text)?;
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Handle>()
            .request(Action::ImportHandoff { profile })
    })
    .await
    .map_err(|e| e.to_string())?
}
