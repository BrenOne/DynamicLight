use crate::settings::{save_json, Settings};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug)]
pub enum SystemEvent {
    Ready,
    Sleep(bool),
    DisplayOff(bool),
    Locked(bool),
    SourcesChanged,
    #[cfg(target_os = "linux")]
    AudioChanged,
    Warning(String),
}

/// User intent is independent of temporary OS/USB/capture availability.
#[derive(Default)]
pub struct Lifecycle {
    pub initializing: bool,
    pub requested: bool,
    pub capture_allowed: bool,
    pub sleeping: bool,
    pub display_off: bool,
    pub locked: bool,
}
impl Lifecycle {
    pub fn pause_reason(&self, s: &Settings) -> &'static str {
        if self.initializing {
            "Checking desktop state"
        } else if self.sleeping {
            "Computer sleeping"
        } else if self.locked && s.off_on_lock {
            "Computer locked"
        } else if self.display_off && s.off_on_display_sleep {
            "Display asleep"
        } else {
            ""
        }
    }
    pub fn active(&self, s: &Settings) -> bool {
        self.requested && self.pause_reason(s).is_empty()
    }
    pub fn interrupted(&mut self, s: &Settings) {
        self.capture_allowed &= s.resume_capture;
    }
    pub fn stop(&mut self) {
        self.requested = false;
        self.capture_allowed = false;
    }
}

#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SavedIntent {
    pub running: bool,
}
impl SavedIntent {
    pub fn load(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            return Ok(Self::default());
        }
        serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| format!("Could not read previous lighting state: {e}"))
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        save_json(self, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overlapping_events_and_stop_do_not_resurrect_lighting() {
        let s = Settings::default();
        let mut life = Lifecycle {
            requested: true,
            capture_allowed: true,
            ..Default::default()
        };
        life.locked = true;
        life.display_off = true;
        life.sleeping = true;
        assert!(!life.active(&s));
        life.sleeping = false;
        assert!(!life.active(&s));
        life.locked = false;
        assert!(!life.active(&s));
        life.stop();
        life.display_off = false;
        assert!(!life.active(&s));
        assert!(!life.capture_allowed);
    }
    #[test]
    fn independent_preferences_and_capture_consent() {
        let mut s = Settings {
            off_on_lock: false,
            resume_capture: false,
            ..Default::default()
        };
        let mut life = Lifecycle {
            requested: true,
            capture_allowed: true,
            locked: true,
            ..Default::default()
        };
        assert!(life.active(&s));
        life.display_off = true;
        assert!(!life.active(&s));
        life.interrupted(&s);
        assert!(!life.capture_allowed);
        s.resume_capture = true;
        life.interrupted(&s);
        assert!(
            !life.capture_allowed,
            "a preference change cannot authorize a stopped capture"
        );
        life.capture_allowed = true;
        life.interrupted(&s);
        assert!(life.capture_allowed);
    }
    #[test]
    fn stopped_intent_survives_restart_and_corruption_is_not_on() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("run-state.json");
        assert!(!SavedIntent::load(&path).unwrap().running);
        SavedIntent { running: true }.save(&path).unwrap();
        assert!(SavedIntent::load(&path).unwrap().running);
        SavedIntent { running: false }.save(&path).unwrap();
        assert!(!SavedIntent::load(&path).unwrap().running);
        std::fs::write(&path, b"invalid").unwrap();
        assert!(SavedIntent::load(&path).is_err());
    }
}
