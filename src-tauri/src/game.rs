//! Counter-Strike 2 Game State Integration, explicitly enabled loopback HTTP only.
use crate::{
    bounded_http as http,
    engine::{Action, Handle},
    extensions::{Overlay, Rule},
    plugins::Plugins,
};
use dynamiclight_control::session::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    net::{Ipv4Addr, TcpListener},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tauri::Manager;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    controller: String,
    rules: Vec<Rule>,
}
impl Config {
    fn validate(&self) -> Result<(), String> {
        if self.rules.len() > 8 {
            return Err("At most eight telemetry rules".into());
        }
        for rule in &self.rules {
            rule.validate()?;
        }
        Ok(())
    }
}
struct Session {
    active: Arc<AtomicBool>,
    connection: Connection,
    config: Config,
    seen: Arc<Mutex<Option<Instant>>>,
}
pub struct Game {
    session: Mutex<Option<Session>>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    enabled: bool,
    connected: bool,
    endpoint: Option<String>,
    config: Option<Config>,
}
impl Game {
    fn status(&self) -> Status {
        let session = self.session.lock().unwrap();
        Status {
            enabled: session.is_some(),
            connected: session.as_ref().is_some_and(|s| {
                s.seen
                    .lock()
                    .unwrap()
                    .is_some_and(|t| t.elapsed() < Duration::from_secs(10))
            }),
            endpoint: session
                .as_ref()
                .map(|s| format!("http://127.0.0.1:{}/gsi", s.connection.port)),
            config: session.as_ref().map(|s| s.config.clone()),
        }
    }
}
#[derive(Default)]
struct Frame {
    health: Option<u64>,
    kills: Option<u64>,
    round: String,
    bomb: String,
}
fn frame(value: &Value) -> Option<Frame> {
    if value.pointer("/provider/appid").and_then(Value::as_u64) != Some(730) {
        return None;
    }
    let phase = value.pointer("/map/phase").and_then(Value::as_str)?;
    if !["live", "warmup", "intermission"].contains(&phase) {
        return None;
    }
    let own = value.pointer("/provider/steamid").and_then(Value::as_str)?;
    if own.is_empty()
        || Some(own) != value.pointer("/player/steamid").and_then(Value::as_str)
        || value.pointer("/player/activity").and_then(Value::as_str) != Some("playing")
    {
        return None;
    }
    Some(Frame {
        health: value
            .pointer("/player/state/health")
            .and_then(Value::as_u64)
            .filter(|v| *v <= 100),
        kills: value
            .pointer("/player/state/round_kills")
            .and_then(Value::as_u64)
            .filter(|v| *v <= 100),
        round: value
            .pointer("/round/phase")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .chars()
            .take(24)
            .collect(),
        bomb: value
            .pointer("/round/bomb")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .chars()
            .take(24)
            .collect(),
    })
}
fn events(previous: &Frame, current: &Frame) -> Vec<&'static str> {
    let mut events = vec![];
    if current.round == "live" && previous.round != "live" {
        events.push("roundStart");
    }
    if previous
        .health
        .zip(current.health)
        .is_some_and(|(a, b)| b < a)
    {
        events.push("damage");
    }
    if previous
        .kills
        .zip(current.kills)
        .is_some_and(|(a, b)| b > a)
    {
        events.push("kill");
    }
    if current.bomb == "planted" && previous.bomb != "planted" {
        events.push("bombPlanted");
    }
    events
}
pub fn init(app: &tauri::AppHandle) {
    app.manage(Game {
        session: Mutex::new(None),
    });
}
pub fn stop(app: &tauri::AppHandle) {
    if let Some(game) = app.try_state::<Game>() {
        if let Some(session) = game.session.lock().unwrap().take() {
            session.active.store(false, Ordering::Release);
        }
    }
}
#[tauri::command]
pub fn game_status(state: tauri::State<'_, Game>) -> Status {
    state.status()
}
#[tauri::command]
pub async fn game_configure(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    config: Option<Config>,
) -> Result<Status, String> {
    if window.label() != "main" {
        return Err("Configure telemetry in the main window".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let target = if let Some(config) = &config {
            config.validate()?;
            Some(
                app.state::<Handle>()
                    .fleet
                    .as_ref()
                    .unwrap()
                    .target(&config.controller)?,
            )
        } else {
            None
        };
        stop(&app);
        if let (Some(config), Some(target)) = (config, target) {
            let listener =
                TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).map_err(|e| e.to_string())?;
            listener.set_nonblocking(true).map_err(|e| e.to_string())?;
            let connection =
                Connection::new(listener.local_addr().map_err(|e| e.to_string())?.port())?;
            let active = Arc::new(AtomicBool::new(true));
            let seen = Arc::new(Mutex::new(None));
            let state = Session {
                active: active.clone(),
                connection: connection.clone(),
                config: config.clone(),
                seen: seen.clone(),
            };
            let app_copy = app.clone();
            std::thread::Builder::new()
                .name("game-telemetry".into())
                .spawn(move || {
                    let host = format!("127.0.0.1:{}", connection.port);
                    let mut previous = None;
                    let mut cue_guard = Arc::new(AtomicBool::new(true));
                    let mut last = Instant::now();
                    let mut rate = http::Rate::new();
                    while active.load(Ordering::Acquire) {
                        if last.elapsed() > Duration::from_secs(10) {
                            cue_guard.store(false, Ordering::Release);
                            previous = None;
                            *seen.lock().unwrap() = None;
                        }
                        let mut stream = match listener.accept() {
                            Ok((s, _)) => s,
                            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                                std::thread::sleep(Duration::from_millis(20));
                                continue;
                            }
                            Err(_) => break,
                        };
                        if !rate.accept() {
                            http::reply(&mut stream, 429, "text/plain", b"Rate limited", "");
                            continue;
                        }
                        let _ = stream.set_nonblocking(false);
                        let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
                        let _ = stream.set_write_timeout(Some(Duration::from_millis(500)));
                        let result = (|| -> Result<(), String> {
                            let request = http::read(&mut stream, &host)?;
                            if request.method != "POST"
                                || request.path != "/gsi"
                                || request.headers.contains_key("origin")
                            {
                                return Err("Invalid telemetry request".into());
                            }
                            let value: Value = serde_json::from_slice(&request.body)
                                .map_err(|_| "Invalid telemetry JSON")?;
                            if !http::equal(
                                value
                                    .pointer("/auth/token")
                                    .and_then(Value::as_str)
                                    .unwrap_or_default(),
                                &connection.token,
                            ) || !active.load(Ordering::Acquire)
                            {
                                return Err("Unauthorized".into());
                            }
                            let current = frame(&value);
                            if let Some(current) = current {
                                last = Instant::now();
                                *seen.lock().unwrap() = Some(last);
                                if !cue_guard.load(Ordering::Acquire) {
                                    cue_guard = Arc::new(AtomicBool::new(true));
                                }
                                if let Some(old) = &previous {
                                    for event in events(old, &current) {
                                        let mut cues: Vec<_> = config
                                            .rules
                                            .iter()
                                            .filter(|r| r.event == event)
                                            .map(|r| (r.clone(), active.clone()))
                                            .collect();
                                        cues.extend(app_copy.state::<Plugins>().cues(event));
                                        for (rule, guard) in cues {
                                            // Session revocation is checked by the worker as well as the cue lifetime.
                                            let overlay = Overlay::cue(rule, cue_guard.clone())?
                                                .with_guard(guard)
                                                .with_guard(active.clone());
                                            let _ = target.request(Action::Automation {
                                                action: Box::new(Action::Extension {
                                                    overlay,
                                                    cue: true,
                                                }),
                                                active: active.clone(),
                                                allow_capture: false,
                                            });
                                        }
                                    }
                                }
                                previous = Some(current);
                            } else {
                                cue_guard.store(false, Ordering::Release);
                                previous = None;
                                *seen.lock().unwrap() = None;
                            }
                            Ok(())
                        })();
                        http::reply(
                            &mut stream,
                            if result.is_ok() { 200 } else { 400 },
                            "text/plain",
                            if result.is_ok() { b"OK" } else { b"Rejected" },
                            "",
                        );
                    }
                    cue_guard.store(false, Ordering::Release);
                })
                .map_err(|e| e.to_string())?;
            *app.state::<Game>().session.lock().unwrap() = Some(state);
        }
        Ok(app.state::<Game>().status())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub fn game_export(app: tauri::AppHandle, window: tauri::WebviewWindow) -> Result<String, String> {
    if window.label() != "main" {
        return Err("Export setup in the main window".into());
    }
    let state = app.state::<Game>();
    let session = state.session.lock().unwrap();
    let session = session.as_ref().ok_or("Enable telemetry first")?;
    let text=format!("\"DynamicLight\"\n{{\n \"uri\" \"http://127.0.0.1:{}/gsi\"\n \"timeout\" \"2.0\"\n \"buffer\" \"0.1\"\n \"throttle\" \"0.2\"\n \"heartbeat\" \"2.0\"\n \"auth\" {{ \"token\" \"{}\" }}\n \"data\" {{ \"provider\" \"1\" \"map\" \"1\" \"player_id\" \"1\" \"player_state\" \"1\" \"round\" \"1\" }}\n}}\n",session.connection.port,session.connection.token);
    let path = app
        .path()
        .download_dir()
        .map_err(|e| e.to_string())?
        .join("gamestate_integration_dynamiclight.cfg");
    std::fs::write(&path, text).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_owned_active_player_state_drives_bounded_events() {
        let mut value = serde_json::json!({"provider":{"appid":730,"steamid":"own"},"map":{"phase":"live"},"player":{"steamid":"own","activity":"playing","state":{"health":100,"round_kills":0}},"round":{"phase":"freezetime"}});
        let old = frame(&value).unwrap();
        value["player"]["state"]["health"] = 70.into();
        value["player"]["state"]["round_kills"] = 1.into();
        value["round"]["phase"] = "live".into();
        value["round"]["bomb"] = "planted".into();
        assert_eq!(
            events(&old, &frame(&value).unwrap()),
            vec!["roundStart", "damage", "kill", "bombPlanted"]
        );
        value["player"]["steamid"] = "spectated".into();
        assert!(frame(&value).is_none());
        value["player"]["steamid"] = "own".into();
        value["map"]["phase"] = "gameover".into();
        assert!(frame(&value).is_none());
    }
}
