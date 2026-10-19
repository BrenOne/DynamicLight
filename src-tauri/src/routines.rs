//! Local routines. All decisions and temporary output share the engine's single writer.
use crate::settings::{SceneSettings, SourceBinding};
use chrono::{Datelike, Local, TimeZone, Timelike};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    time::{Duration, Instant},
};
use tauri::Manager;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Cue {
    pub enabled: bool,
    pub color: String,
    pub seconds: u8,
}
impl Default for Cue {
    fn default() -> Self {
        Self {
            enabled: false,
            color: "#86b7ff".into(),
            seconds: 3,
        }
    }
}
impl Cue {
    fn validate(&self) -> Result<(), String> {
        if !(1..=10).contains(&self.seconds)
            || self.color.len() != 7
            || !self.color.starts_with('#')
            || !self.color[1..].bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err("Cues need a #RRGGBB color and 1–10 seconds".into());
        }
        Ok(())
    }
    fn rgb(&self) -> [u8; 3] {
        std::array::from_fn(|i| {
            u8::from_str_radix(&self.color[1 + i * 2..3 + i * 2], 16).unwrap_or(0)
        })
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Scene {
    pub name: String,
    pub settings: SceneSettings,
    pub source: SourceBinding,
}
impl Scene {
    fn validate(&self) -> Result<(), String> {
        self.settings.validate()?;
        self.source.capture_source.validate()?;
        if self.name.is_empty() || self.name.len() > 200 || self.source.screen_id.len() > 200 {
            return Err("Invalid routine scene".into());
        }
        // Unattended routines never grant capture consent.
        if matches!(self.settings.mode.as_str(), "screen" | "music" | "blend") {
            return Err("Routines require an offline color, effect or replay scene. Start capture manually.".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Transition {
    Immediate,
    FadeIn,
    FadeOut,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Schedule {
    pub id: String,
    pub enabled: bool,
    pub days: Vec<u8>, // Monday=0
    pub minute: u16,
    pub scene: Scene,
    pub transition: Transition,
    pub seconds: u16,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppRule {
    pub app: String,
    pub scene: Scene,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    pub paused: bool,
    pub schedules: Vec<Schedule>,
    pub app_rules: Vec<AppRule>,
    pub apps_enabled: bool,
    pub quiet_enabled: bool,
    pub quiet_start: u16,
    pub quiet_end: u16,
    pub manual_cue: Cue,
    pub focus_cue: Cue,
    pub break_cue: Cue,
    pub focus_minutes: u16,
    pub break_minutes: u16,
    pub cycles: u8,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            paused: true,
            schedules: vec![],
            app_rules: vec![],
            apps_enabled: false,
            quiet_enabled: false,
            quiet_start: 22 * 60,
            quiet_end: 8 * 60,
            manual_cue: Cue::default(),
            focus_cue: Cue::default(),
            break_cue: Cue::default(),
            focus_minutes: 25,
            break_minutes: 5,
            cycles: 1,
        }
    }
}
impl Config {
    pub fn validate(&self) -> Result<(), String> {
        if self.schedules.len() > 32
            || self.app_rules.len() > 32
            || self.quiet_start >= 1440
            || self.quiet_end >= 1440
            || !(1..=240).contains(&self.focus_minutes)
            || !(1..=120).contains(&self.break_minutes)
            || !(1..=20).contains(&self.cycles)
        {
            return Err("Routine limits: 32 schedules/rules, 1–240 focus minutes, 1–120 break minutes, 1–20 cycles".into());
        }
        for cue in [&self.manual_cue, &self.focus_cue, &self.break_cue] {
            cue.validate()?;
        }
        let mut ids = std::collections::HashSet::new();
        for s in &self.schedules {
            if s.id.is_empty()
                || s.id.len() > 64
                || !s.id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                || !ids.insert(&s.id)
                || s.minute >= 1440
                || s.days.is_empty()
                || s.days.len() > 7
                || s.days.iter().any(|d| *d > 6)
                || !(1..=3600).contains(&s.seconds)
            {
                return Err("Invalid or duplicate schedule".into());
            }
            s.scene.validate()?;
        }
        for rule in &self.app_rules {
            if rule.app.trim().is_empty()
                || rule.app.len() > 500
                || rule.app.chars().any(char::is_control)
            {
                return Err("Choose an application name".into());
            }
            rule.scene.validate()?;
        }
        if serde_json::to_vec(self).map_err(|e| e.to_string())?.len() > 256 * 1024 {
            return Err("Routine configuration must fit within 256 KiB; reduce saved scene complexity or rows".into());
        }
        Ok(())
    }
    pub fn quiet(&self, minute: u16) -> bool {
        self.quiet_enabled
            && if self.quiet_start < self.quiet_end {
                (self.quiet_start..self.quiet_end).contains(&minute)
            } else {
                minute >= self.quiet_start || minute < self.quiet_end
            }
    }
}
#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub scene_revision: u64,
    pub paused: bool,
    pub watch_apps: bool,
    pub next: String,
    pub message: String,
    pub active_rule: String,
    pub timer_phase: String,
    pub remaining_seconds: u64,
    pub cycle: u8,
    pub timer_paused: bool,
    pub cue_active: bool,
}
struct Timer {
    phase: bool,
    cycle: u8,
    remaining: Duration,
    last: Instant,
    paused: bool,
}
pub struct Runtime {
    pub config: Config,
    pub status: Status,
    path: PathBuf,
    fired: BTreeMap<String, String>,
    wall: Option<(i64, i32)>,
    timer: Option<Timer>,
    cue: Option<(Cue, Instant)>,
    queued: Option<Cue>,
    pub foreground: String,
    pub app_override: bool,
}
impl Runtime {
    pub fn load(path: PathBuf) -> Self {
        let config: Config = std::fs::read(&path)
            .ok()
            .filter(|v| v.len() <= 256 * 1024)
            .and_then(|v| serde_json::from_slice(&v).ok())
            .filter(|v: &Config| v.validate().is_ok())
            .unwrap_or_default();
        let fired = std::fs::read(path.with_extension("ledger.json"))
            .ok()
            .filter(|v| v.len() <= 16384)
            .and_then(|v| serde_json::from_slice(&v).ok())
            .unwrap_or_default();
        Self {
            config,
            status: Status::default(),
            path,
            fired,
            wall: None,
            timer: None,
            cue: None,
            queued: None,
            foreground: String::new(),
            app_override: false,
        }
    }
    pub fn configure(&mut self, config: Config) -> Result<(), String> {
        config.validate()?;
        crate::settings::save_json(&config, &self.path)?;
        self.config = config;
        self.cancel_cues();
        self.timer = None;
        self.wall = None;
        self.app_override = false;
        self.foreground.clear();
        self.fired
            .retain(|id, _| self.config.schedules.iter().any(|s| &s.id == id));
        self.status.message =
            "Routines saved. Timer reset; current-minute schedules are skipped.".into();
        Ok(())
    }
    pub fn cancel_cues(&mut self) {
        self.cue = None;
        self.queued = None;
    }
    pub fn suspend(&mut self) {
        self.wall = None;
        self.cancel_cues();
        if let Some(timer) = &mut self.timer {
            timer.paused = true;
            timer.last = Instant::now();
        }
    }
    pub fn manual_override(&mut self) {
        self.app_override = true;
        // The next foreground sample establishes a fresh baseline, so a sample
        // already in flight cannot immediately undo the user's change.
        self.foreground.clear();
        self.status.active_rule.clear();
        self.status.message = "Manual override. Application rules resume when focus changes; schedules keep their chosen pause setting.".into();
        self.cancel_cues();
    }
    fn enqueue(&mut self, cue: Cue, active: bool, minute: u16, now: Instant) {
        if !cue.enabled || !active || self.config.quiet(minute) {
            return;
        }
        // One pending event: newest wins; no unbounded queue or notification text.
        if self.cue.is_some() {
            self.queued = Some(cue);
        } else {
            self.cue = Some((cue, now));
        }
    }
    pub fn command(&mut self, command: &str, active: bool) -> Result<(), String> {
        let now = Instant::now();
        match command {
            "status" => {}
            "pauseSchedules" | "resumeSchedules" => {
                let mut config = self.config.clone();
                config.paused = command == "pauseSchedules";
                crate::settings::save_json(&config, &self.path)?;
                self.config = config;
                self.wall = None;
            }
            "cue" => self.enqueue(self.config.manual_cue.clone(), active, local_minute(), now),
            "start" => {
                self.timer = Some(Timer {
                    phase: false,
                    cycle: 1,
                    remaining: Duration::from_secs(u64::from(self.config.focus_minutes) * 60),
                    last: now,
                    paused: false,
                });
            }
            "pause" => {
                if let Some(t) = &mut self.timer {
                    if !t.paused {
                        t.remaining = t
                            .remaining
                            .saturating_sub(now.saturating_duration_since(t.last));
                    }
                    t.paused = !t.paused;
                    t.last = now;
                }
            }
            "skip" => {
                if let Some(t) = &mut self.timer {
                    t.remaining = Duration::ZERO;
                    t.paused = false;
                    t.last = now;
                }
            }
            "cancel" => {
                self.timer = None;
                self.cancel_cues();
            }
            "resumeRules" => {
                self.app_override = false;
                self.foreground.clear();
            }
            _ => return Err("Unknown routine control".into()),
        }
        Ok(())
    }
    pub fn tick_timer(&mut self, now: Instant, available: bool, active: bool, minute: u16) {
        let mut boundary = None;
        if let Some(t) = &mut self.timer {
            let elapsed = now.saturating_duration_since(t.last);
            t.last = now;
            // Large gaps (suspend/blocked worker) pause instead of racing through cycles.
            if !available || elapsed > Duration::from_secs(5) {
                t.paused = true;
            }
            if !t.paused {
                t.remaining = t.remaining.saturating_sub(elapsed);
            }
            if t.remaining.is_zero() && !t.paused {
                if t.phase && t.cycle >= self.config.cycles {
                    boundary = Some(self.config.focus_cue.clone());
                    self.timer = None;
                } else {
                    t.phase = !t.phase;
                    if !t.phase {
                        t.cycle += 1;
                    }
                    t.remaining = Duration::from_secs(
                        u64::from(if t.phase {
                            self.config.break_minutes
                        } else {
                            self.config.focus_minutes
                        }) * 60,
                    );
                    boundary = Some(if t.phase {
                        self.config.break_cue.clone()
                    } else {
                        self.config.focus_cue.clone()
                    });
                }
            }
        }
        if !active || !available || self.config.quiet(minute) {
            self.cancel_cues();
        }
        if self.cue.as_ref().is_some_and(|(c, start)| {
            now.saturating_duration_since(*start) >= Duration::from_secs(c.seconds.into())
        }) {
            self.cue = self.queued.take().map(|c| (c, now));
        }
        if let Some(cue) = boundary {
            self.enqueue(cue, active && available, minute, now);
        }
        self.status.timer_phase = self
            .timer
            .as_ref()
            .map(|t| if t.phase { "Break" } else { "Focus" }.into())
            .unwrap_or_default();
        self.status.remaining_seconds = self
            .timer
            .as_ref()
            .map(|t| {
                t.remaining
                    .as_secs()
                    .saturating_add(u64::from(t.remaining.subsec_nanos() > 0))
            })
            .unwrap_or(0);
        self.status.cycle = self.timer.as_ref().map(|t| t.cycle).unwrap_or(0);
        self.status.timer_paused = self.timer.as_ref().is_some_and(|t| t.paused);
        self.status.cue_active = self.cue.is_some();
        self.status.paused = self.config.paused;
        self.status.watch_apps = self.config.apps_enabled && !self.config.app_rules.is_empty();
    }
    pub fn overlay(&self, colors: &mut [[u8; 3]]) {
        if let Some((cue, _)) = &self.cue {
            colors.fill(cue.rgb());
        }
    }
    pub fn schedule(&mut self, now: chrono::DateTime<Local>, available: bool) -> Option<Schedule> {
        let current = (now.timestamp() / 60, now.offset().local_minus_utc());
        if self.wall == Some(current) {
            return None;
        }
        let previous = self.wall.replace(current);
        let minute = (now.hour() * 60 + now.minute()) as u16;
        let date = now.date_naive().to_string();
        let due = !self.config.paused
            && available
            && previous.is_some_and(|p| p.1 == current.1 && current.0 - p.0 == 1);
        self.status.next = next_schedule(&self.config, now);
        if !due {
            return None;
        }
        // First row wins if several schedules share a time; mark all as consumed.
        let matches: Vec<_> = self
            .config
            .schedules
            .iter()
            .filter(|s| {
                s.enabled
                    && s.minute == minute
                    && s.days
                        .contains(&(now.weekday().num_days_from_monday() as u8))
                    && self.fired.get(&s.id) != Some(&date)
            })
            .cloned()
            .collect();
        for s in &matches {
            self.fired.insert(s.id.clone(), date.clone());
        }
        if !matches.is_empty() {
            if let Err(e) =
                crate::settings::save_json(&self.fired, &self.path.with_extension("ledger.json"))
            {
                self.status.message = format!("Schedule skipped: {e}");
                return None;
            }
        }
        matches.into_iter().next()
    }
}
pub fn local_minute() -> u16 {
    let now = Local::now();
    (now.hour() * 60 + now.minute()) as u16
}
fn next_schedule(config: &Config, now: chrono::DateTime<Local>) -> String {
    if config.paused {
        return "Schedules paused".into();
    }
    let mut next = Vec::new();
    for offset in 0..=7 {
        let date = now.date_naive() + chrono::Duration::days(offset);
        for s in &config.schedules {
            if !s.enabled
                || !s
                    .days
                    .contains(&(date.weekday().num_days_from_monday() as u8))
            {
                continue;
            }
            let local = date
                .and_hms_opt(u32::from(s.minute / 60), u32::from(s.minute % 60), 0)
                .unwrap();
            if let Some(at) = Local
                .from_local_datetime(&local)
                .earliest()
                .filter(|at| *at > now)
            {
                next.push((at, s));
            }
        }
    }
    next.sort_by_key(|v| v.0);
    next.first()
        .map(|(at, s)| format!("{} · {}", at.format("%a %d %b %H:%M %:z"), s.scene.name))
        .unwrap_or_else(|| "No scheduled actions".into())
}

#[tauri::command]
pub async fn routines_config(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    config: Option<Config>,
) -> Result<Config, String> {
    if window.label() != "main" {
        return Err("Open the main window to configure routines".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let result = app
            .state::<crate::engine::Handle>()
            .request(crate::engine::Action::RoutinesConfig { config })?;
        Ok(result.routines_config)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn routine_apps() -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        crate::capture::check_session()?;
        let mut names = std::collections::BTreeSet::new();
        for w in xcap::Window::all()
            .map_err(|e| e.to_string())?
            .into_iter()
            .take(500)
        {
            if let Ok(name) = w.app_name() {
                if !name.trim().is_empty() && name.len() <= 500 {
                    names.insert(name);
                }
            }
        }
        Ok(names.into_iter().collect())
    })
    .await
    .map_err(|e| e.to_string())?
}
pub fn init(app: &tauri::AppHandle) -> Result<(), String> {
    let handle = app.state::<crate::engine::Handle>().inner().clone();
    std::thread::Builder::new()
        .name("application-rules".into())
        .spawn(move || loop {
            std::thread::sleep(Duration::from_secs(1));
            let Ok(snapshot) = handle.request(crate::engine::Action::Snapshot) else {
                break;
            };
            if !snapshot.routines.watch_apps {
                continue;
            }
            let result = foreground();
            let _ = handle.request(crate::engine::Action::Foreground { result });
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}
fn foreground() -> Result<String, String> {
    crate::capture::check_session()?;
    let windows = xcap::Window::all().map_err(|e| e.to_string())?;
    for w in windows.into_iter().take(500) {
        if w.is_focused().map_err(|e| e.to_string())? {
            return w.app_name().map_err(|e| e.to_string());
        }
    }
    Ok(String::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn runtime() -> (tempfile::TempDir, Runtime) {
        let dir = tempfile::tempdir().unwrap();
        let r = Runtime::load(dir.path().join("routines.json"));
        (dir, r)
    }
    #[test]
    fn cues_are_bounded_coalesced_and_honor_off_and_overnight_quiet_hours() {
        let (_dir, mut r) = runtime();
        let start = Instant::now();
        let cue = Cue {
            enabled: true,
            color: "#ff0000".into(),
            seconds: 2,
        };
        r.enqueue(cue.clone(), false, 600, start);
        assert!(r.cue.is_none());
        r.config.quiet_enabled = true;
        r.enqueue(cue.clone(), true, 23 * 60, start);
        assert!(r.cue.is_none());
        assert!(r.config.quiet(60));
        assert!(!r.config.quiet(12 * 60));
        r.enqueue(cue.clone(), true, 600, start);
        let mut newest = cue.clone();
        newest.color = "#00ff00".into();
        r.enqueue(cue, true, 600, start);
        r.enqueue(newest, true, 600, start);
        r.tick_timer(start + Duration::from_secs(2), true, true, 600);
        let mut colors = [[0; 3]; 2];
        r.overlay(&mut colors);
        assert_eq!(colors, [[0, 255, 0]; 2]);
        assert!(r.queued.is_none());
        r.tick_timer(start + Duration::from_secs(3), true, false, 600);
        assert!(r.cue.is_none());
        r.config.quiet_end = r.config.quiet_start;
        assert!(r.config.quiet(600));
    }
    #[test]
    fn focus_timer_cycles_skip_cancel_and_suspend_without_catchup() {
        let (_dir, mut r) = runtime();
        r.config.cycles = 2;
        r.config.break_cue.enabled = true;
        r.command("start", false).unwrap();
        let now = r.timer.as_ref().unwrap().last;
        r.tick_timer(now + Duration::from_secs(2), true, false, 600);
        assert_eq!(r.status.remaining_seconds, 25 * 60 - 2);
        r.tick_timer(now + Duration::from_secs(20), true, false, 600);
        assert!(r.status.timer_paused);
        assert_eq!(r.status.remaining_seconds, 25 * 60 - 2);
        r.command("skip", false).unwrap();
        r.tick_timer(Instant::now(), true, false, 600);
        assert_eq!(r.status.timer_phase, "Break");
        assert!(!r.status.cue_active);
        r.command("skip", false).unwrap();
        r.tick_timer(Instant::now(), true, false, 600);
        assert_eq!(r.status.cycle, 2);
        assert_eq!(r.status.timer_phase, "Focus");
        r.command("cancel", false).unwrap();
        r.tick_timer(Instant::now(), true, false, 600);
        assert!(r.status.timer_phase.is_empty());
    }
    #[test]
    fn schedule_deduplicates_local_date_and_skips_startup_missed_sleep_and_timezone_changes() {
        let (dir, mut r) = runtime();
        let now = Local::now()
            .date_naive()
            .and_hms_opt(12, 0, 0)
            .unwrap()
            .and_local_timezone(Local)
            .earliest()
            .unwrap();
        r.config.paused = false;
        r.config.schedules.push(Schedule {
            id: "daily".into(),
            enabled: true,
            days: (0..7).collect(),
            minute: 720,
            scene: Scene {
                name: "Test".into(),
                settings: SceneSettings::capture(&crate::settings::Settings::default()),
                source: SourceBinding::default(),
            },
            transition: Transition::Immediate,
            seconds: 10,
        });
        r.configure(r.config.clone()).unwrap();
        assert!(r.schedule(now, true).is_none()); // Startup never catches up.
        r.wall = Some((now.timestamp() / 60 - 2, now.offset().local_minus_utc()));
        assert!(r.schedule(now, true).is_none());
        r.wall = Some((
            now.timestamp() / 60 - 1,
            now.offset().local_minus_utc() + 3600,
        ));
        assert!(r.schedule(now, true).is_none());
        r.wall = Some((now.timestamp() / 60 - 1, now.offset().local_minus_utc()));
        assert!(r.schedule(now, false).is_none());
        r.wall = Some((now.timestamp() / 60 - 1, now.offset().local_minus_utc()));
        assert!(r.schedule(now, true).is_some());
        let mut restarted = Runtime::load(dir.path().join("routines.json"));
        restarted.wall = Some((now.timestamp() / 60 - 1, now.offset().local_minus_utc()));
        assert!(restarted.schedule(now, true).is_none()); // Repeated hour/restart deduplicates.
        r.suspend();
        assert!(r.schedule(now + chrono::Duration::days(1), true).is_none());
    }
    #[test]
    fn configuration_rejects_capture_and_invalid_bounds() {
        let mut config = Config {
            cycles: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
        config.cycles = 1;
        config.manual_cue.color = "#😀".into();
        assert!(config.validate().is_err());
        let s = crate::settings::Settings {
            mode: "screen".into(),
            ..Default::default()
        };
        let scene = Scene {
            name: "Capture".into(),
            settings: SceneSettings::capture(&s),
            source: SourceBinding::default(),
        };
        assert!(scene.validate().is_err());
    }
}
