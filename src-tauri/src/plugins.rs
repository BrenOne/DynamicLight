//! Plugin ABI v1 is declarative JSON, never native code, JavaScript or WASI.
use crate::{
    engine::{Action, Handle},
    extensions::{Overlay, Rule},
    studio::Composition,
};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use tauri::Manager;

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum Program {
    Effect { composition: Composition },
    Integration { rules: Vec<Rule> },
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Manifest {
    pub api_version: u8,
    pub id: String,
    pub name: String,
    pub version: String,
    pub capabilities: Vec<String>,
    pub program: Program,
}
impl Manifest {
    fn validate(&self) -> Result<(), String> {
        if self.api_version != 1
            || self.id.is_empty()
            || self.id.len() > 64
            || !self
                .id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            || self.name.trim().is_empty()
            || self.name.len() > 100
            || self.name.chars().any(char::is_control)
            || self.version.is_empty()
            || self.version.len() > 30
            || !self
                .version
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b".-".contains(&b))
        {
            return Err("Invalid plugin v1 identity".into());
        }
        let required: &[&str] = match &self.program {
            Program::Effect { composition } => {
                composition.validate()?;
                &["lighting"]
            }
            Program::Integration { rules } => {
                if rules.is_empty() || rules.len() > 8 {
                    return Err("Integration needs 1–8 rules".into());
                }
                for rule in rules {
                    rule.validate()?;
                }
                &["lighting", "gameEvents"]
            }
        };
        if self
            .capabilities
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            != required
        {
            return Err(
                "Capabilities must exactly match the plugin kind; unsupported access is denied"
                    .into(),
            );
        }
        Ok(())
    }
    fn parse(text: &str) -> Result<Self, String> {
        if text.len() > 32768 {
            return Err("Plugin limit is 32 KiB".into());
        }
        let manifest: Self =
            serde_json::from_str(text).map_err(|e| format!("Invalid plugin: {e}"))?;
        manifest.validate()?;
        Ok(manifest)
    }
}
struct Entry {
    manifest: Manifest,
    active: Arc<AtomicBool>,
}
pub struct Plugins {
    entries: Mutex<Vec<Entry>>,
    path: PathBuf,
}
#[derive(Serialize)]
pub struct View {
    manifest: Manifest,
    enabled: bool,
}
impl Plugins {
    fn view(entries: &[Entry]) -> Vec<View> {
        entries
            .iter()
            .map(|e| View {
                manifest: e.manifest.clone(),
                enabled: e.active.load(Ordering::Acquire),
            })
            .collect()
    }
    pub fn cues(&self, event: &str) -> Vec<(Rule, Arc<AtomicBool>)> {
        self.entries
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.active.load(Ordering::Acquire))
            .flat_map(|e| match &e.manifest.program {
                Program::Integration { rules } => rules
                    .iter()
                    .filter(|r| r.event == event)
                    .map(|r| (r.clone(), e.active.clone()))
                    .collect(),
                _ => Vec::new(),
            })
            .collect()
    }
}
pub fn init(app: &tauri::AppHandle) -> Result<(), String> {
    let path = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("plugins.json");
    let manifests: Vec<Manifest> = std::fs::metadata(&path)
        .ok()
        .filter(|m| m.len() <= 524288)
        .and_then(|_| std::fs::read(&path).ok())
        .and_then(|b| serde_json::from_slice::<Vec<Manifest>>(&b).ok())
        .filter(|m| m.len() <= 16 && m.iter().all(|p| p.validate().is_ok()))
        .unwrap_or_default();
    app.manage(Plugins {
        path,
        entries: Mutex::new(
            manifests
                .into_iter()
                .map(|manifest| Entry {
                    manifest,
                    active: Arc::new(AtomicBool::new(false)),
                })
                .collect(),
        ),
    });
    Ok(())
}
#[tauri::command]
pub async fn plugin_manage(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    operation: String,
    id: Option<String>,
    text: Option<String>,
    controller: Option<String>,
    consent: Option<bool>,
) -> Result<serde_json::Value, String> {
    if window.label() != "main" {
        return Err("Manage plugins in the main window".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let state=app.state::<Plugins>();
        if operation=="preview" { return serde_json::to_value(Manifest::parse(text.as_deref().unwrap_or_default())?).map_err(|e|e.to_string()); }
        let mut entries=state.entries.lock().unwrap();
        match operation.as_str() {
            "list" => {},
            "install" => {
                let manifest=Manifest::parse(text.as_deref().unwrap_or_default())?;
                if entries.len()>=16 || entries.iter().any(|e|e.manifest.id==manifest.id) { return Err("Maximum 16 plugins; remove an existing ID before installing its replacement".into()); }
                let mut saved: Vec<_>=entries.iter().map(|e|e.manifest.clone()).collect();
                saved.push(manifest.clone()); crate::settings::save_json(&saved,&state.path)?;
                entries.push(Entry {manifest,active:Arc::new(AtomicBool::new(false))});
            },
            "enable" | "disable" | "remove" | "activate" => {
                let index=entries.iter().position(|e|Some(e.manifest.id.as_str())==id.as_deref()).ok_or("Unknown plugin")?;
                if operation=="remove" {
                    let saved:Vec<_>=entries.iter().enumerate().filter(|(i,_)|*i!=index).map(|(_,e)|&e.manifest).collect();
                    crate::settings::save_json(&saved,&state.path)?;
                    entries[index].active.store(false,Ordering::Release); entries.remove(index);
                } else {
                    let entry=&mut entries[index];
                    match operation.as_str() {
                        "disable" => entry.active.store(false,Ordering::Release),
                        "enable" => {
                            if matches!(entry.manifest.program,Program::Integration{..}) && consent!=Some(true) {return Err("Explicit permission to read numeric game events is required".into());}
                            entry.active.store(false,Ordering::Release);
                            entry.active=Arc::new(AtomicBool::new(true));
                        },
                        _ => {
                            if !entry.active.load(Ordering::Acquire) {return Err("Enable this plugin first".into());}
                            let Program::Effect {composition}=&entry.manifest.program else {return Err("Integration plugins respond to enabled game telemetry".into());};
                            let target=app.state::<Handle>().fleet.as_ref().unwrap().target(controller.as_deref().ok_or("Choose a controller")?)?;
                            target.request(Action::Extension {overlay:Overlay::effect(composition.clone(),entry.active.clone())?,cue:false})?;
                        }
                    }
                }
            },
            _ => return Err("Unknown plugin operation".into())
        }
        serde_json::to_value(Plugins::view(&entries)).map_err(|e|e.to_string())
    }).await.map_err(|e|e.to_string())?
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn manifest_rejects_code_capabilities_versions_and_unbounded_programs() {
        let mut m = Manifest {
            api_version: 1,
            id: "example".into(),
            name: "Example".into(),
            version: "1.0.0".into(),
            capabilities: vec!["lighting".into()],
            program: Program::Effect {
                composition: Composition::default(),
            },
        };
        assert!(m.validate().is_ok());
        m.capabilities.push("filesystem".into());
        assert!(m.validate().is_err());
        m.capabilities.pop();
        m.api_version = 2;
        assert!(m.validate().is_err());
        m.api_version = 1;
        let mut value = serde_json::to_value(&m).unwrap();
        value["script"] = serde_json::json!("alert(1)");
        assert!(Manifest::parse(&value.to_string()).is_err());
        assert!(Manifest::parse(&" ".repeat(32769)).is_err());
    }
}
