//! Allowlisted diagnostics: never persist source names, paths, raw errors or captured data.
use crate::{
    engine::{Action, Handle, Snapshot},
    settings::save_json,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::Manager;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
enum Health {
    #[default]
    Stopped,
    Paused,
    CapturePaused,
    Reconnecting,
    UsbError,
    CaptureError,
    Attention,
    Sending,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct State {
    connected: bool,
    requested: bool,
    capturing: bool,
    health: Health,
}
impl State {
    fn from(s: &Snapshot) -> Self {
        let health = if !s.pause_reason.is_empty() {
            Health::Paused
        } else if s.error.starts_with("USB write failed:") {
            Health::UsbError
        } else if s.capture_requires_start {
            Health::CapturePaused
        } else if s.recovering {
            Health::Reconnecting
        } else if s.error.contains("Capture") || s.error.contains("capture") {
            Health::CaptureError
        } else if !s.error.is_empty() || !s.lifecycle_warning.is_empty() {
            Health::Attention
        } else if !s.requested {
            Health::Stopped
        } else {
            Health::Sending
        };
        Self {
            connected: s.status.connected,
            requested: s.requested,
            capturing: s.capturing,
            health,
        }
    }
    fn guidance(&self) -> &'static str {
        match self.health {
            Health::Paused => "Unlock or wake the computer. Stop cancels pending recovery.",
            Health::CapturePaused | Health::CaptureError => "Check the selected source and OS recording permissions, then choose Retry.",
            Health::UsbError | Health::Reconnecting => "Check the USB connection and close other strip-control apps. Retry; Stop cancels recovery.",
            Health::Attention => "Read the app's visible error. Check source permissions and retry the affected operation.",
            Health::Stopped => "Lighting is stopped. Choose Start when ready.",
            Health::Sending => "Check physical LEDs separately; USB writes do not confirm illumination.",
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    unix_seconds: u64,
    state: State,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Log {
    enabled: bool,
    entries: VecDeque<Entry>,
}
#[derive(Clone)]
pub struct Diagnostics {
    log: Arc<Mutex<Log>>,
    path: PathBuf,
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
const LIMIT: usize = 200;
impl Diagnostics {
    pub fn new(path: PathBuf) -> Self {
        let log: Log = std::fs::metadata(&path)
            .ok()
            .filter(|m| m.len() <= 100_000)
            .and_then(|_| std::fs::read(&path).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let this = Self {
            log: Arc::new(Mutex::new(log)),
            path,
        };
        {
            let mut log = this.log.lock().unwrap();
            Self::trim(&mut log);
        }
        this
    }
    fn trim(log: &mut Log) {
        let cutoff = now().saturating_sub(7 * 24 * 60 * 60);
        log.entries
            .retain(|e| e.unix_seconds >= cutoff && e.unix_seconds <= now());
        while log.entries.len() > LIMIT {
            log.entries.pop_front();
        }
        if !log.enabled {
            log.entries.clear();
        }
    }
    pub fn observe(&self, s: &Snapshot) {
        // Never delay lighting on disk I/O; the next snapshot retries the observation.
        let Ok(mut log) = self.log.try_lock() else {
            return;
        };
        Self::trim(&mut log);
        let state = State::from(s);
        if log.enabled && log.entries.back().is_none_or(|e| e.state != state) {
            log.entries.push_back(Entry {
                unix_seconds: now(),
                state,
            });
            Self::trim(&mut log);
        }
    }
    pub fn start_writer(&self) {
        let this = self.clone();
        std::thread::spawn(move || {
            let mut previous = String::new();
            loop {
                std::thread::sleep(Duration::from_secs(5));
                // Hold the lock through persistence so Clear/Disable cannot be undone by a stale write.
                let mut log = this.log.lock().unwrap();
                Self::trim(&mut log);
                let signature = serde_json::to_string(&*log).unwrap_or_default();
                if signature != previous && save_json(&*log, &this.path).is_ok() {
                    previous = signature;
                }
            }
        });
    }
    fn report(&self, s: &Snapshot) -> Result<Report, String> {
        let mut log = self.log.lock().map_err(|e| e.to_string())?;
        Self::trim(&mut log);
        let state = State::from(s);
        let text = format!("DynamicLight {} · {} / {}\nGenerated: {} (Unix seconds)\n\nConnected: {}\nRequested: {}\nCapturing: {}\nState: {:?}\nUSB FPS: {}\nFrames sent: {}\nWrite failures: {}\nMissed frame slots: {}\nLast write duration: {:.2} ms\n\n{}\n\nMissed slots count skipped engine deadlines while animation/capture is active, not video-source drops or deliberate idle frames. Counters reset when the app restarts.\n\nLocal history: {} (up to 200 state changes / 7 days)\n{}\n\nExcluded: raw errors, paths, identifiers, profile names, screen content and audio.\n", env!("CARGO_PKG_VERSION"), std::env::consts::OS, std::env::consts::ARCH, now(), state.connected, state.requested, state.capturing, state.health, s.fps, s.frames_written, s.write_errors, s.dropped_frames, s.write_duration_ms, state.guidance(), if log.enabled { "enabled" } else { "disabled" }, serde_json::to_string_pretty(&log.entries).map_err(|e| e.to_string())?);
        Ok(Report {
            text,
            enabled: log.enabled,
        })
    }
}
#[derive(Serialize)]
pub struct Report {
    text: String,
    enabled: bool,
}
#[tauri::command]
pub async fn diagnostic_report(
    app: tauri::AppHandle,
    enabled: Option<bool>,
    clear: bool,
) -> Result<Report, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let diagnostics = app.state::<Diagnostics>();
        if enabled.is_some() || clear {
            let mut log = diagnostics.log.lock().map_err(|e| e.to_string())?;
            let mut next = log.clone();
            if let Some(enabled) = enabled {
                next.enabled = enabled;
            }
            if clear || !next.enabled {
                next.entries.clear();
            }
            save_json(&next, &diagnostics.path)?;
            *log = next;
        }
        let snapshot = app.state::<Handle>().request(Action::Snapshot)?;
        diagnostics.report(&snapshot)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn export_diagnostics(app: tauri::AppHandle) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let snapshot = app.state::<Handle>().request(Action::Snapshot)?;
        let report = app.state::<Diagnostics>().report(&snapshot)?;
        let directory = app.path().download_dir().map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        let file = tempfile::Builder::new()
            .prefix("DynamicLight-diagnostics-")
            .suffix(".txt")
            .tempfile_in(directory)
            .map_err(|e| e.to_string())?;
        use std::io::Write;
        file.as_file()
            .write_all(report.text.as_bytes())
            .map_err(|e| e.to_string())?;
        let (_, path) = file.keep().map_err(|e| e.to_string())?;
        Ok(path.to_string_lossy().into_owned())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn log_is_bounded_and_only_allowlisted_states_reach_export() {
        let dir = tempfile::tempdir().unwrap();
        let d = Diagnostics::new(dir.path().join("diagnostics.json"));
        d.log.lock().unwrap().enabled = true;
        let mut e = crate::engine::Engine::new(
            Default::default(),
            dir.path().join("settings.json"),
            "secret C:/Users/private serial-123".into(),
        );
        for i in 0..250 {
            e.state.requested = i % 2 == 0;
            d.observe(&e.state);
        }
        let report = d.report(&e.state).unwrap();
        assert_eq!(d.log.lock().unwrap().entries.len(), LIMIT);
        assert!(!report.text.contains("private") && !report.text.contains("serial-123"));
        d.log.lock().unwrap().enabled = false;
        d.observe(&e.state);
        assert!(d.log.lock().unwrap().entries.is_empty());
    }
}
