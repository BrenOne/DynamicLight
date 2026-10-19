//! Opt-in local automation. The renderer can configure it but never receives its token.
use crate::{
    engine::{Action, Handle, Snapshot},
    profiles::{Profile, Store},
};
use dynamiclight_control::{http, session::Connection, Command, Power};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    net::{Ipv4Addr, TcpListener},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use tauri::{Emitter, Manager};

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Preferences {
    enabled: bool,
    allow_capture: bool,
}
struct Session {
    active: Arc<AtomicBool>,
    port: u16,
}
struct Model {
    preferences: Preferences,
    session: Option<Session>,
    message: String,
}
pub struct Automation {
    model: Mutex<Model>,
    path: PathBuf,
    connection: PathBuf,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    enabled: bool,
    allow_capture: bool,
    endpoint: Option<String>,
    connection_file: String,
    message: String,
}
impl Automation {
    fn status(&self, model: &Model) -> Status {
        Status {
            enabled: model.session.is_some(),
            allow_capture: model.preferences.allow_capture,
            endpoint: model
                .session
                .as_ref()
                .map(|s| format!("http://127.0.0.1:{}/v1/command", s.port)),
            connection_file: self.connection.to_string_lossy().into_owned(),
            message: model.message.clone(),
        }
    }
    fn revoke(&self, model: &mut Model) {
        if let Some(session) = model.session.take() {
            session.active.store(false, Ordering::Release);
        }
        let _ = std::fs::remove_file(&self.connection);
    }
    fn start(&self, app: &tauri::AppHandle, model: &mut Model) -> Result<(), String> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).map_err(|e| e.to_string())?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        let connection = Connection::new(port)?;
        connection.save(&self.connection)?;
        let active = Arc::new(AtomicBool::new(true));
        let guard = active.clone();
        let app = app.clone();
        let capture = model.preferences.allow_capture;
        if let Err(e) = http::serve(listener, connection, active.clone(), move |command| {
            dispatch(&app, command, guard.clone(), capture)
        }) {
            let _ = std::fs::remove_file(&self.connection);
            return Err(e.to_string());
        }
        model.session = Some(Session { active, port });
        Ok(())
    }
}
pub fn init(app: &tauri::AppHandle) -> Result<(), String> {
    let directory = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let path = directory.join("automation-preferences.json");
    let preferences: Preferences = std::fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    let automation = Automation {
        model: Mutex::new(Model {
            preferences,
            session: None,
            message: String::new(),
        }),
        path,
        connection: directory.join("automation/session.json"),
    };
    {
        let mut model = automation.model.lock().unwrap();
        automation.revoke(&mut model); // Remove discovery left by an unclean exit.
        if model.preferences.enabled {
            if let Err(e) = automation.start(app, &mut model) {
                model.message = format!("Local automation could not start: {e}");
            }
        }
    }
    app.manage(automation);
    Ok(())
}
pub fn stop(app: &tauri::AppHandle) {
    if let Some(state) = app.try_state::<Automation>() {
        state.revoke(&mut state.model.lock().unwrap());
    }
}
#[tauri::command]
pub fn automation_status(state: tauri::State<'_, Automation>) -> Status {
    state.status(&state.model.lock().unwrap())
}
#[tauri::command]
pub async fn automation_configure(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    enabled: bool,
    allow_capture: bool,
) -> Result<Status, String> {
    if window.label() != "main" {
        return Err("Configure automation in the main window".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<Automation>();
        let mut model = state.model.lock().unwrap();
        let preferences = Preferences {
            enabled,
            allow_capture,
        };
        crate::settings::save_json(&preferences, &state.path)?;
        state.revoke(&mut model);
        model.preferences = preferences;
        model.message.clear();
        if enabled {
            if let Err(e) = state.start(&app, &mut model) {
                model.message = format!("Local automation could not start: {e}");
            }
        }
        Ok(state.status(&model))
    })
    .await
    .map_err(|e| e.to_string())?
}
fn metadata(profiles: &[Profile]) -> Value {
    json!(profiles
        .iter()
        .map(|p| json!({"id":p.id, "name":p.name, "mode":p.settings.mode}))
        .collect::<Vec<_>>())
}
fn status(snapshot: &Snapshot) -> Value {
    json!({"appVersion":env!("CARGO_PKG_VERSION"), "connected":snapshot.status.connected,
        "requested":snapshot.requested, "powered":snapshot.powered, "mode":snapshot.settings.mode,
        "brightness":snapshot.settings.brightness, "capturing":snapshot.capturing,
        "captureRequiresStart":snapshot.capture_requires_start, "pauseReason":snapshot.pause_reason,
        "calibrating":snapshot.calibrating, "error":snapshot.error})
}
fn dispatch(
    app: &tauri::AppHandle,
    command: Command,
    active: Arc<AtomicBool>,
    capture: bool,
) -> Result<Value, String> {
    command.validate()?;
    let (action, allow_capture) = match command {
        Command::Routine { action } => {
            let snapshot = app.state::<Handle>().request(Action::Automation {
                action: Box::new(Action::Routine {
                    command: action.command().into(),
                }),
                active,
                allow_capture: false,
            })?;
            return serde_json::to_value(&snapshot.routines).map_err(|e| e.to_string());
        }
        Command::Status {} => {
            return app
                .state::<Handle>()
                .request(Action::Snapshot)
                .map(|s| status(&s))
        }
        Command::Scenes {} => return Ok(metadata(&crate::scenes::starter_scenes().profiles)),
        Command::Profiles {} => {
            return Ok(metadata(
                &app.state::<Store>().library.lock().unwrap().profiles,
            ))
        }
        Command::Brightness { value } => (
            Action::Adjust {
                brightness: Some(value),
                color: None,
            },
            false,
        ),
        Command::Power {
            state,
            allow_capture,
        } => (
            match state {
                Power::On => Action::Start,
                Power::Off => Action::Power { on: false },
                Power::Toggle => Action::Toggle,
            },
            allow_capture,
        ),
        Command::ApplyScene { id, allow_capture } => {
            let p = crate::scenes::starter_scenes()
                .profiles
                .into_iter()
                .find(|p| p.id == id)
                .ok_or("Unknown scene ID")?;
            (
                Action::ApplyProfile {
                    scene: p.settings,
                    source: p.source,
                },
                allow_capture,
            )
        }
        Command::ApplyProfile { id, allow_capture } => {
            let p = app
                .state::<Store>()
                .library
                .lock()
                .unwrap()
                .profiles
                .iter()
                .find(|p| p.id == id)
                .cloned()
                .ok_or("Unknown profile ID")?;
            (
                Action::ApplyProfile {
                    scene: p.settings,
                    source: p.source,
                },
                allow_capture,
            )
        }
    };
    let snapshot = app.state::<Handle>().request(Action::Automation {
        action: Box::new(action),
        active,
        allow_capture: capture && allow_capture,
    })?;
    let _ = app.emit("external-settings", &snapshot);
    Ok(status(&snapshot))
}
