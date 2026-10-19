//! A bounded fleet of independent single-writer engines. No USB handle is shared.
use crate::{
    engine::{Action, Handle, Snapshot},
    settings::{save_json, Settings},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tauri::{Emitter, Manager};
type Emit = Arc<dyn Fn(&Snapshot) + Send + Sync>;
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_routes_cannot_escape_storage_or_duplicate_slots() {
        let mut config = Config::default();
        assert!(config.validate());
        config.slots.push(Slot {
            key: "../outside".into(),
            name: "Bad".into(),
        });
        assert!(!config.validate());
        config.slots[1].key = "primary".into();
        assert!(!config.validate());
        config.slots[1].key = "second".into();
        assert!(config.validate());
        config
            .positions
            .insert("screen".into(), Position { x: 100001, y: 0 });
        assert!(!config.validate());
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Slot {
    key: String,
    name: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    selected: String,
    slots: Vec<Slot>,
    #[serde(default)]
    positions: BTreeMap<String, Position>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Position {
    x: i32,
    y: i32,
}
fn valid_positions(positions: &BTreeMap<String, Position>) -> bool {
    positions.len() <= 16
        && positions.iter().all(|(id, p)| {
            id.len() <= 200
                && (-100000..=100000).contains(&p.x)
                && (-100000..=100000).contains(&p.y)
        })
}
impl Default for Config {
    fn default() -> Self {
        Self {
            positions: BTreeMap::new(),
            selected: "primary".into(),
            slots: vec![Slot {
                key: "primary".into(),
                name: "Main strip".into(),
            }],
        }
    }
}
impl Config {
    fn validate(&self) -> bool {
        let mut ids = std::collections::HashSet::new();
        (1..=8).contains(&self.slots.len())
            && valid_positions(&self.positions)
            && self.slots[0].key == "primary"
            && self.slots.iter().any(|s| s.key == self.selected)
            && self.slots.iter().all(|s| {
                !s.key.is_empty()
                    && s.key.len() <= 80
                    && s.key
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-')
                    && ids.insert(&s.key)
                    && !s.name.trim().is_empty()
                    && s.name.chars().count() <= 50
                    && !s.name.chars().any(char::is_control)
            })
    }
}
struct Model {
    config: Config,
    handles: BTreeMap<String, Handle>,
}
pub struct Fleet {
    live: Arc<Mutex<std::collections::HashSet<String>>>,
    model: Mutex<Model>,
    selected: Arc<Mutex<String>>,
    cache: Arc<Mutex<BTreeMap<String, Snapshot>>>,
    directory: PathBuf,
    emit: Emit,
}
impl Fleet {
    fn path(&self, key: &str) -> PathBuf {
        if key == "primary" {
            self.directory.join("settings.json")
        } else {
            self.directory
                .join("controllers")
                .join(key)
                .join("settings.json")
        }
    }
    fn spawn(&self, key: String, settings: Settings, warning: String) -> Handle {
        self.live.lock().unwrap().insert(key.clone());
        let live = self.live.clone();
        let selected = self.selected.clone();
        let cache = self.cache.clone();
        let emit = self.emit.clone();
        let path = self.path(&key);
        crate::engine::start(settings, path, warning, move |snapshot| {
            let live = live.lock().unwrap();
            if !live.contains(&key) {
                return;
            }
            cache.lock().unwrap().insert(key.clone(), snapshot.clone());
            if *selected.lock().unwrap() == key {
                emit(snapshot);
            }
        })
    }
    fn selected(&self) -> Handle {
        let model = self.model.lock().unwrap();
        model.handles[&model.config.selected].clone()
    }
    pub(crate) fn all(&self) -> Vec<Handle> {
        self.model
            .lock()
            .unwrap()
            .handles
            .values()
            .cloned()
            .collect()
    }
    pub fn request(&self, action: Action) -> Result<Snapshot, String> {
        if matches!(action, Action::Shutdown | Action::PrepareUpdate) {
            let update = matches!(action, Action::PrepareUpdate);
            let mut last = None;
            let mut errors = Vec::new();
            for h in self.all() {
                match h.request(if update {
                    Action::PrepareUpdate
                } else {
                    Action::Shutdown
                }) {
                    Ok(s) => last = Some(s),
                    Err(e) => errors.push(e),
                }
            }
            if !errors.is_empty() {
                return Err(errors.join("; "));
            }
            return last.ok_or("No controller workers".into());
        }
        if let Action::Foreground { result } = action {
            for h in self.all() {
                let _ = h.request(Action::Foreground {
                    result: result.clone(),
                });
            }
            return self.selected().request(Action::Snapshot);
        }
        let mut snapshot = self.selected().request(action)?;
        // One foreground watcher serves every opted-in controller, including hidden ones.
        snapshot.routines.watch_apps |= self
            .cache
            .lock()
            .unwrap()
            .values()
            .any(|s| s.routines.watch_apps);
        Ok(snapshot)
    }
    pub fn system_event(&self, event: crate::lifecycle::SystemEvent, wait: bool) {
        for h in self.all() {
            h.system_event(event.clone(), wait);
        }
    }
    fn save(&self, config: &Config) -> Result<(), String> {
        if !config.validate() {
            return Err("Use 1–8 controllers, unique slots and names of 1–50 characters".into());
        }
        save_json(config, &self.directory.join("controllers.json"))
    }
    fn select(&self, key: &str) -> Result<(), String> {
        let mut model = self.model.lock().unwrap();
        if !model.handles.contains_key(key) {
            return Err("Unknown controller".into());
        }
        let mut config = model.config.clone();
        config.selected = key.into();
        self.save(&config)?;
        model.config = config;
        *self.selected.lock().unwrap() = key.into();
        Ok(())
    }
    fn add(&self, id: String, name: String) -> Result<(), String> {
        let device = crate::device::choices()?
            .into_iter()
            .find(|d| d.id == id)
            .ok_or("Controller unavailable. Refresh USB devices.")?;
        let existing: Vec<_> = self
            .all()
            .into_iter()
            .map(|h| h.request(Action::Snapshot))
            .collect::<Result<_, _>>()?;
        if existing.iter().any(|s| s.settings.device_id.is_empty()) {
            return Err(
                "Bind the main strip to a USB interface before adding another controller".into(),
            );
        }
        if existing
            .iter()
            .any(|s| device.aliases.contains(&s.settings.device_id))
        {
            return Err("This interface already has a controller".into());
        }
        let key = format!(
            "strip-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        let mut model = self.model.lock().unwrap();
        let mut config = model.config.clone();
        config.slots.push(Slot {
            key: key.clone(),
            name,
        });
        config.selected = key.clone();
        if !config.validate() {
            return Err("Keep up to eight controllers and use a name of 1–50 characters".into());
        }
        let settings = Settings {
            device_id: id,
            ..Settings::default()
        };
        settings.save(&self.path(&key))?;
        self.save(&config)?;
        let handle = self.spawn(key.clone(), settings, String::new());
        handle.system_event(crate::lifecycle::SystemEvent::Ready, false);
        model.handles.insert(key.clone(), handle);
        model.config = config;
        *self.selected.lock().unwrap() = key;
        Ok(())
    }
    fn remove(&self, key: &str) -> Result<(), String> {
        if key == "primary" {
            return Err("The main strip cannot be removed; rebind it instead".into());
        }
        let handle = {
            self.model
                .lock()
                .unwrap()
                .handles
                .get(key)
                .cloned()
                .ok_or("Unknown controller")?
        };
        let mut model = self.model.lock().unwrap();
        let mut config = model.config.clone();
        config.slots.retain(|s| s.key != key);
        if config.selected == key {
            config.selected = "primary".into();
        }
        self.save(&config)?;
        // Persist the removal before stopping; failure leaves a retryable live slot.
        let _ = handle.request(Action::Power { on: false });
        let stopped = handle.request(Action::Shutdown);
        *self.selected.lock().unwrap() = config.selected.clone();
        model.config = config;
        model.handles.remove(key);
        self.live.lock().unwrap().remove(key);
        self.cache.lock().unwrap().remove(key);
        stopped.map(|_| ())
    }
    fn view(&self) -> View {
        let model = self.model.lock().unwrap();
        let cache = self.cache.lock().unwrap();
        View {
            positions: model.config.positions.clone(),
            selected: model.config.selected.clone(),
            controllers: model
                .config
                .slots
                .iter()
                .map(|slot| Row {
                    key: slot.key.clone(),
                    name: slot.name.clone(),
                    state: cache.get(&slot.key).cloned(),
                })
                .collect(),
        }
    }
}
#[derive(Serialize)]
pub struct Row {
    key: String,
    name: String,
    state: Option<Snapshot>,
}
#[derive(Serialize)]
pub struct View {
    positions: BTreeMap<String, Position>,
    selected: String,
    controllers: Vec<Row>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Route {
    key: String,
    screen_id: String,
    count: usize,
    sides: u8,
    offset: usize,
    reverse: bool,
}
#[tauri::command]
pub async fn controller_routes(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    routes: Vec<Route>,
    positions: BTreeMap<String, Position>,
) -> Result<View, String> {
    if window.label() != "main" {
        return Err("Manage routing in the main window".into());
    }
    tauri::async_runtime::spawn_blocking(move||{
        if routes.len()>8 || !valid_positions(&positions){return Err("Invalid layout size or monitor coordinates".into());}
        let fleet=fleet(&app);let mut keys=std::collections::HashSet::new();
        let screens=crate::capture::screens().unwrap_or_default();let mut changes=Vec::new();
        for route in routes {
            if !keys.insert(route.key.clone()){return Err("A controller may appear only once".into());}
            let handle=fleet.model.lock().unwrap().handles.get(&route.key).cloned().ok_or("Unknown controller")?;
            let current=handle.request(Action::Snapshot)?;
            if current.calibrating {return Err("Finish all strip calibrations before applying routes".into());}
            if route.screen_id!=current.settings.screen_id && !route.screen_id.is_empty() && !screens.iter().any(|s|s.id==route.screen_id){return Err("Selected display is unavailable. Refresh the layout.".into());}
            let patch=crate::settings::SettingsPatch(serde_json::from_value(serde_json::json!({"screenId":route.screen_id,"count":route.count,"sides":route.sides,"offset":route.offset,"reverse":route.reverse})).map_err(|e|e.to_string())?);
            patch.apply(&current.settings)?;changes.push((handle,patch));
        }
        let mut config=fleet.model.lock().unwrap().config.clone();config.positions=positions;fleet.save(&config)?;
        fleet.model.lock().unwrap().config=config;
        let mut errors=Vec::new();
        for (handle,patch) in changes {if let Err(e)=handle.request(Action::PatchSettings{patch,persist:true}){errors.push(e);}}
        if let Ok(snapshot)=fleet.selected().request(Action::Snapshot){let _=app.emit("external-settings",snapshot);}
        if !errors.is_empty(){return Err(format!("Some routes could not be applied; refresh to review: {}",errors.join("; ")));}
        Ok(fleet.view())
    }).await.map_err(|e|e.to_string())?
}
pub fn start(
    settings: Settings,
    path: PathBuf,
    warning: String,
    emit: impl Fn(&Snapshot) + Send + Sync + 'static,
) -> Handle {
    let directory = path.parent().unwrap().to_path_buf();
    let config: Config = std::fs::read(directory.join("controllers.json"))
        .ok()
        .filter(|v| v.len() <= 16384)
        .and_then(|v| serde_json::from_slice(&v).ok())
        .filter(Config::validate)
        .unwrap_or_default();
    let fleet = Arc::new(Fleet {
        live: Arc::new(Mutex::new(std::collections::HashSet::new())),
        selected: Arc::new(Mutex::new(config.selected.clone())),
        cache: Arc::new(Mutex::new(BTreeMap::new())),
        directory,
        emit: Arc::new(emit),
        model: Mutex::new(Model {
            config: config.clone(),
            handles: BTreeMap::new(),
        }),
    });
    let primary = fleet.spawn("primary".into(), settings, warning);
    fleet
        .model
        .lock()
        .unwrap()
        .handles
        .insert("primary".into(), primary.clone());
    for slot in config.slots.iter().skip(1) {
        // Corrupt/missing bindings never fall back to automatic hardware selection.
        let (settings, warning) = match Settings::load(&fleet.path(&slot.key)) {
            Ok(s) if !s.device_id.is_empty() => (s, String::new()),
            _ => (
                Settings {
                    device_id: "p-dead".into(),
                    ..Default::default()
                },
                "Saved controller settings unavailable; rebind explicitly".into(),
            ),
        };
        let handle = fleet.spawn(slot.key.clone(), settings, warning);
        fleet
            .model
            .lock()
            .unwrap()
            .handles
            .insert(slot.key.clone(), handle);
    }
    primary.with_fleet(fleet)
}
fn fleet(app: &tauri::AppHandle) -> Arc<Fleet> {
    app.state::<Handle>().fleet.as_ref().unwrap().clone()
}
#[tauri::command]
pub async fn controller_devices() -> Result<Vec<crate::device::Choice>, String> {
    tauri::async_runtime::spawn_blocking(crate::device::choices)
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub fn controller_list(app: tauri::AppHandle) -> View {
    fleet(&app).view()
}
#[tauri::command]
pub async fn controller_manage(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    operation: String,
    key: String,
    name: String,
) -> Result<View, String> {
    if window.label() != "main" {
        return Err("Manage controllers in the main window".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let fleet = fleet(&app);
        match operation.as_str() {
            "select" => fleet.select(&key)?,
            "add" => fleet.add(key, name)?,
            "remove" => fleet.remove(&key)?,
            "bind" => {
                let device = crate::device::choices()?
                    .into_iter()
                    .find(|d| d.id == key)
                    .ok_or("Controller unavailable. Refresh USB devices.")?;
                let selected = fleet.selected();
                let current = selected.request(Action::Snapshot)?;
                for h in fleet.all() {
                    let s = h.request(Action::Snapshot)?;
                    if device.aliases.contains(&s.settings.device_id)
                        && s.settings.device_id != current.settings.device_id
                    {
                        return Err("This USB interface is already assigned".into());
                    }
                }
                selected.request(Action::BindController { id: key })?;
            }
            _ => return Err("Unknown controller operation".into()),
        }
        let snapshot = fleet.selected().request(Action::Snapshot)?;
        let _ = app.emit("external-settings", &snapshot);
        Ok(fleet.view())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn controller_group(
    app: tauri::AppHandle,
    operation: String,
    value: u8,
) -> Result<View, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let fleet = fleet(&app);
        let mut errors = Vec::new();
        if !["start", "stop", "brightness"].contains(&operation.as_str()) || value > 100 {
            return Err("Invalid group command".into());
        }
        for h in fleet.all() {
            if operation == "start" && h.request(Action::Snapshot)?.handoff_released {
                continue;
            }
            let action = match operation.as_str() {
                "start" => Action::Start,
                "stop" => Action::Power { on: false },
                _ => Action::GroupBrightness { value },
            };
            if let Err(e) = h.request(action) {
                errors.push(e);
            }
        }
        if !errors.is_empty() {
            return Err(errors.join("; "));
        }
        Ok(fleet.view())
    })
    .await
    .map_err(|e| e.to_string())?
}
