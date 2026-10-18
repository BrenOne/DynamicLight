//! Profile persistence and shortcuts. Hardware and lifecycle preferences stay local.
use crate::{
    engine::{Action, Handle, Snapshot},
    settings::{save_json, Settings},
};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, path::PathBuf, sync::Mutex};
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub favorite: bool,
    pub shortcut: String,
    pub settings: Settings,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Library {
    pub profiles: Vec<Profile>,
}
pub struct Store {
    pub library: Mutex<Library>,
    path: PathBuf,
}
impl Library {
    pub fn validate(&self) -> Result<(), String> {
        if self.profiles.len() > 40 {
            return Err("Keep up to 40 profiles".into());
        }
        let mut ids = HashSet::new();
        let mut shortcuts = HashSet::new();
        for p in &self.profiles {
            if p.id.is_empty()
                || p.id.len() > 64
                || !p.id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                || !ids.insert(&p.id)
            {
                return Err("Invalid or duplicate profile ID".into());
            }
            if p.name.trim().is_empty()
                || p.name.chars().count() > 50
                || p.name.chars().any(char::is_control)
            {
                return Err("Use a profile name of 1–50 characters".into());
            }
            p.settings.validate()?;
            if !p.shortcut.is_empty() {
                if p.shortcut.len() > 80 {
                    return Err("Shortcut is too long".into());
                }
                let key = p
                    .shortcut
                    .parse::<Shortcut>()
                    .map_err(|e| format!("Invalid shortcut: {e}"))?;
                if key.mods.is_empty() {
                    return Err("Shortcuts need a modifier, such as Control or Alt".into());
                }
                if !shortcuts.insert(key.id()) {
                    return Err("Two profiles use the same shortcut".into());
                }
            }
        }
        Ok(())
    }
}
pub fn scene(profile: &Settings, current: &Settings) -> Settings {
    Settings {
        count: current.count,
        sides: current.sides,
        reverse: current.reverse,
        offset: current.offset,
        off_on_sleep: current.off_on_sleep,
        off_on_display_sleep: current.off_on_display_sleep,
        off_on_lock: current.off_on_lock,
        off_on_quit: current.off_on_quit,
        restore_on_start: current.restore_on_start,
        resume_capture: current.resume_capture,
        max_brightness: current.max_brightness,
        transition_limit: current.transition_limit,
        minimum_glow: current.minimum_glow,
        ..profile.clone()
    }
}
pub fn apply(app: &tauri::AppHandle, id: &str) -> Result<Snapshot, String> {
    let p = app
        .state::<Store>()
        .library
        .lock()
        .map_err(|e| e.to_string())?
        .profiles
        .iter()
        .find(|p| p.id == id)
        .cloned()
        .ok_or("Profile no longer exists")?;
    let handle = app.state::<Handle>();
    // Deliberate profile selection is a Start; OS permission checks still apply.
    let state = handle.request(Action::ApplyProfile {
        settings: p.settings,
    })?;
    let _ = app.emit("external-settings", &state);
    Ok(state)
}
fn register(app: &tauri::AppHandle, p: &Profile) -> Result<(), String> {
    if app
        .try_state::<tauri_plugin_global_shortcut::GlobalShortcut<tauri::Wry>>()
        .is_none()
    {
        return Err("Global shortcuts are unavailable in this desktop session. Clear the shortcut to save the profile.".into());
    }
    let id = p.id.clone();
    app.global_shortcut()
        .on_shortcut(p.shortcut.as_str(), move |app, _, event| {
            if event.state == ShortcutState::Pressed {
                let app = app.clone();
                let id = id.clone();
                tauri::async_runtime::spawn_blocking(move || {
                    if let Err(error) = apply(&app, &id) {
                        let _ = app.emit("control-error", error);
                    }
                });
            }
        })
        .map_err(|e| format!("Shortcut {} is unavailable: {e}", p.shortcut))
}
fn unregister(app: &tauri::AppHandle, key: &str) {
    if app
        .try_state::<tauri_plugin_global_shortcut::GlobalShortcut<tauri::Wry>>()
        .is_some()
    {
        let _ = app.global_shortcut().unregister(key);
    }
}
pub fn init(app: &tauri::AppHandle) -> Result<(), String> {
    let path = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("profiles.json");
    let loaded = (|| -> Result<Library, String> {
        if !path.exists() {
            return Ok(Library::default());
        }
        let library: Library =
            serde_json::from_slice(&std::fs::read(&path).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        library.validate()?;
        Ok(library)
    })();
    let library = loaded.unwrap_or_else(|e| {
        app.state::<Handle>().system_event(crate::lifecycle::SystemEvent::Warning(format!("Profiles could not be loaded: {e}. The original file is unchanged until you save a profile.")), false);
        Library::default()
    });
    app.manage(Store {
        library: Mutex::new(library.clone()),
        path,
    });
    for p in &library.profiles {
        if !p.shortcut.is_empty() {
            if let Err(e) = register(app, p) {
                app.state::<Handle>()
                    .system_event(crate::lifecycle::SystemEvent::Warning(e), false);
            }
        }
    }
    Ok(())
}
#[tauri::command]
pub async fn profile_library(
    app: tauri::AppHandle,
    library: Option<Library>,
) -> Result<Library, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let store = app.state::<Store>();
        let mut old = store.library.lock().map_err(|e| e.to_string())?;
        if let Some(next) = library {
            next.validate()?;
            // Register additions before touching old bindings or the saved file.
            let unchanged = |p: &Profile| {
                old.profiles
                    .iter()
                    .any(|o| o.id == p.id && o.shortcut == p.shortcut)
            };
            let additions: Vec<_> = next
                .profiles
                .iter()
                .filter(|p| !p.shortcut.is_empty() && !unchanged(p))
                .collect();
            let mut added: Vec<&Profile> = vec![];
            for p in additions {
                if let Err(e) = register(&app, p) {
                    for p in added {
                        unregister(&app, p.shortcut.as_str());
                    }
                    return Err(e);
                }
                added.push(p);
            }
            if let Err(e) = save_json(&next, &store.path) {
                for p in added {
                    unregister(&app, p.shortcut.as_str());
                }
                return Err(e);
            }
            for p in &old.profiles {
                if !p.shortcut.is_empty()
                    && !next
                        .profiles
                        .iter()
                        .any(|n| n.id == p.id && n.shortcut == p.shortcut)
                {
                    unregister(&app, p.shortcut.as_str());
                }
            }
            *old = next;
            let _ = app.emit("profiles-changed", &*old);
        }
        Ok(old.clone())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn apply_profile(app: tauri::AppHandle, id: String) -> Result<Snapshot, String> {
    tauri::async_runtime::spawn_blocking(move || apply(&app, &id))
        .await
        .map_err(|e| e.to_string())?
}
#[cfg(test)]
mod tests {
    use super::*;
    fn profile() -> Profile {
        Profile {
            id: "one".into(),
            name: "Desk".into(),
            favorite: true,
            shortcut: "Control+Shift+D".into(),
            settings: Settings::default(),
        }
    }
    #[test]
    fn duplicate_shortcuts_and_bad_names_rejected() {
        let mut a = profile();
        let mut b = profile();
        b.id = "two".into();
        assert!(Library {
            profiles: vec![a.clone(), b]
        }
        .validate()
        .is_err());
        a.name = "\n".into();
        assert!(Library { profiles: vec![a] }.validate().is_err());
    }
    #[test]
    fn profile_keeps_hardware_and_privacy_choices() {
        let p = Settings {
            count: 2,
            brightness: 80,
            restore_on_start: true,
            ..Default::default()
        };
        let current = Settings::default();
        let applied = scene(&p, &current);
        assert_eq!(applied.count, current.count);
        assert!(!applied.restore_on_start);
        assert_eq!(applied.brightness, 80);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("profiles.json");
        let library = Library {
            profiles: vec![profile()],
        };
        save_json(&library, &path).unwrap();
        let loaded: Library = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        loaded.validate().unwrap();
        assert_eq!(loaded.profiles[0].name, "Desk");
    }
}
