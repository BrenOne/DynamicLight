use crate::{
    audio::Audio,
    capture::ScreenCapture,
    device::{Controller, Status},
    effects,
    lifecycle::{Lifecycle, SavedIntent, SystemEvent},
    lighting,
    protocol::Rgb,
    settings::{SceneSettings, Settings, SettingsPatch, SourceBinding},
};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver, SyncSender},
    thread,
    time::{Duration, Instant},
};

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum Action {
    #[serde(skip)]
    Remote {
        action: Box<Action>,
        active: std::sync::Arc<std::sync::atomic::AtomicBool>,
    },
    #[serde(skip)]
    Extension {
        overlay: crate::extensions::Overlay,
        cue: bool,
    },
    #[serde(skip)]
    AmbientConfig {
        config: Option<crate::ambient::Config>,
    },
    #[serde(skip)]
    AmbientReading {
        source: String,
        result: Result<f32, String>,
    },
    AmbientResume,
    #[serde(skip)]
    ExportHandoff {
        path: PathBuf,
    },
    #[serde(skip)]
    ImportHandoff {
        profile: crate::handoff::Profile,
    },
    #[serde(skip)]
    BindController {
        id: String,
    },
    #[serde(skip)]
    GroupBrightness {
        value: u8,
    },
    #[serde(skip)]
    RoutinesConfig {
        config: Option<crate::routines::Config>,
    },
    Routine {
        command: String,
    },
    #[serde(skip)]
    Foreground {
        result: Result<String, String>,
    },
    #[serde(skip)]
    Automation {
        action: Box<Action>,
        active: std::sync::Arc<std::sync::atomic::AtomicBool>,
        allow_capture: bool,
    },
    RecordStart {
        seconds: u8,
    },
    RecordStop {},
    #[serde(skip)]
    LoadSequence {
        sequence: crate::sequence::Sequence,
    },
    #[serde(skip)]
    ExportSequence {
        path: PathBuf,
    },
    Snapshot,
    SelectMode {
        mode: String,
    },
    StartMode {
        mode: String,
    },
    #[serde(skip)]
    PreviewBrightness {
        brightness: u8,
    },
    #[serde(skip)]
    ApplyProfile {
        scene: SceneSettings,
        source: SourceBinding,
    },
    #[serde(skip)]
    Adjust {
        brightness: Option<u8>,
        color: Option<String>,
    },
    #[serde(skip)]
    Toggle,
    Start,
    CalibrationStart,
    CalibrationStep {
        settings: Settings,
        led: Option<usize>,
    },
    CalibrationEnd {
        apply: bool,
    },
    Connect,
    Disconnect,
    PatchSettings {
        patch: SettingsPatch,
        persist: bool,
    },
    Settings {
        settings: Settings,
    },
    PreviewSettings {
        settings: Settings,
    },
    Power {
        on: bool,
    },
    StartCapture,
    StopCapture,
    #[serde(skip)]
    PrepareUpdate,
    #[serde(skip)]
    Shutdown,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub ambient: crate::ambient::Status,
    pub handoff_released: bool,
    pub group_brightness: u8,
    pub routines: crate::routines::Status,
    #[serde(skip)]
    pub routines_config: crate::routines::Config,
    pub settings: Settings,
    pub sequence: crate::sequence::Status,
    pub status: Status,
    pub powered: bool,
    pub requested: bool,
    pub pause_reason: String,
    pub recovering: bool,
    pub capture_requires_start: bool,
    pub lifecycle_warning: String,
    pub lifecycle_ready: bool,
    pub capturing: bool,
    pub colors: Vec<Rgb>,
    pub fps: u32,
    pub error: String,
    pub capture_size: (u32, u32),
    pub audio_samples: u64,
    pub frames_written: u64,
    pub audio_bands: Vec<f32>,
    pub audio_level: f32,
    pub last_write_ms: Option<u64>,
    pub write_duration_ms: f64,
    pub write_errors: u64,
    pub dropped_frames: u64,
    pub beat_level: f32,
    pub calibrating: bool,
    pub calibration_led: Option<usize>,
}
pub struct Message {
    action: Action,
    reply: SyncSender<Result<Snapshot, String>>,
}
#[derive(Clone)]
pub struct Handle {
    pub(crate) fleet: Option<std::sync::Arc<crate::controllers::Fleet>>,
    tx: SyncSender<Message>,
    events: mpsc::Sender<(SystemEvent, SyncSender<()>)>,
}
impl Handle {
    pub(crate) fn with_fleet(mut self, fleet: std::sync::Arc<crate::controllers::Fleet>) -> Self {
        self.fleet = Some(fleet);
        self
    }
    pub fn request(&self, action: Action) -> Result<Snapshot, String> {
        if let Some(fleet) = &self.fleet {
            return fleet.request(action);
        }
        let (tx, rx) = mpsc::sync_channel(1);
        self.tx
            .try_send(Message { action, reply: tx })
            .map_err(|e| format!("Lighting engine busy or closed: {e}"))?;
        rx.recv()
            .map_err(|_| "Lighting engine closed".to_string())?
    }
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    pub fn power_event(&self, sleeping: bool) {
        self.system_event(SystemEvent::Sleep(sleeping), sleeping);
    }
    pub fn system_event(&self, event: SystemEvent, wait: bool) {
        if let Some(fleet) = &self.fleet {
            fleet.system_event(event, wait);
            return;
        }
        let (tx, rx) = mpsc::sync_channel(1);
        // Lifecycle messages must not be dropped when UI edits fill the command queue.
        if self.events.send((event, tx)).is_ok() && wait {
            let _ = rx.recv_timeout(Duration::from_secs(2));
        }
    }
}
struct CalibrationSession {
    original: Settings,
    requested: bool,
    capture_allowed: bool,
    started: Instant,
}
pub struct Engine {
    extension: Option<crate::extensions::Overlay>,
    external_cue: Option<crate::extensions::Overlay>,
    pub state: Snapshot,
    controller: Controller,
    path: PathBuf,
    desired: bool,
    life: Lifecycle,
    capture_retry: Instant,
    source_check: Instant,
    source_signature: Option<String>,
    blend_sources: (Option<String>, Option<String>),
    screen: Option<ScreenCapture>,
    audio: Option<Audio>,
    previous: Vec<[f32; 3]>,
    started: Instant,
    retry: Instant,
    last_solid: Option<(Rgb, usize)>,
    heartbeat: Instant,
    fps_start: Instant,
    fps_frames: u32,
    last_write: Option<Instant>,
    last_tick: Instant,
    beat: crate::beat::Detector,
    calibration: Option<CalibrationSession>,
    sequences: crate::sequence::Deck,
    routines: crate::routines::Runtime,
    ambient: crate::ambient::Runtime,
    routine_restore: Option<(SceneSettings, SourceBinding, bool)>,
    fade: Option<(Instant, Duration, bool)>,
}
impl Engine {
    pub fn new(settings: Settings, path: PathBuf, warning: String) -> Self {
        let mut controller = Controller::new();
        controller.bind(settings.device_id.clone());
        let group_brightness = std::fs::read(path.with_file_name("group-level.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<u8>(&b).ok())
            .filter(|v| *v <= 100)
            .unwrap_or(100);
        let sequences = crate::sequence::Deck::load(path.with_file_name("lighting-sequence.json"));
        let routines = crate::routines::Runtime::load(path.with_file_name("routines.json"));
        let ambient = crate::ambient::Runtime::load(path.with_file_name("ambient.json"));
        Self {
            state: Snapshot {
                ambient: ambient.status.clone(),
                handoff_released: std::fs::read(path.with_file_name("handoff-released.json"))
                    .ok()
                    .and_then(|b| serde_json::from_slice(&b).ok())
                    .unwrap_or(false),
                group_brightness,
                routines: routines.status.clone(),
                routines_config: routines.config.clone(),
                settings,
                sequence: sequences.status(),
                status: controller.status.clone(),
                powered: false,
                requested: false,
                pause_reason: String::new(),
                recovering: false,
                capture_requires_start: false,
                lifecycle_warning: String::new(),
                lifecycle_ready: false,
                capturing: false,
                colors: vec![],
                fps: 0,
                error: warning,
                capture_size: (0, 0),
                audio_samples: 0,
                frames_written: 0,
                audio_bands: vec![0.0; 32],
                audio_level: 0.0,
                last_write_ms: None,
                write_duration_ms: 0.0,
                write_errors: 0,
                dropped_frames: 0,
                beat_level: 0.0,
                calibrating: false,
                calibration_led: None,
            },
            controller,
            sequences,
            routines,
            ambient,
            routine_restore: None,
            fade: None,
            extension: None,
            external_cue: None,
            path,
            desired: false,
            life: Lifecycle::default(),
            capture_retry: Instant::now(),
            source_check: Instant::now(),
            source_signature: None,
            blend_sources: (None, None),
            screen: None,
            audio: None,
            previous: vec![],
            started: Instant::now(),
            retry: Instant::now(),
            last_solid: None,
            heartbeat: Instant::now(),
            fps_start: Instant::now(),
            fps_frames: 0,
            last_write: None,
            last_tick: Instant::now(),
            beat: crate::beat::Detector::default(),
            calibration: None,
        }
    }
    /// Connect and adopt the LED count the controller reports about itself.
    fn connect_strip(&mut self) -> Result<(), String> {
        self.controller.connect()?;
        if self.state.calibrating {
            return Ok(());
        }
        let mut changed = false;
        if let Some((leds, inches)) = self.controller.detected {
            let n = leds as usize;
            if self.state.settings.count != n {
                self.state.settings.count = n;
                self.previous.clear();
                self.state.colors = vec![[0; 3]; n];
                changed = true;
            }
            // Only a strip that reported its size drives the automatic layout.
            let aspect = crate::capture::display_aspect(&self.state.settings.screen_id)
                .unwrap_or(16.0 / 9.0);
            changed |= lighting::auto_sides(&mut self.state.settings, aspect, Some(inches));
        }
        if changed {
            let _ = self.state.settings.save(&self.path);
        }
        Ok(())
    }
    fn black(&mut self) {
        self.previous.clear();
        self.last_solid = None;
        self.state.colors = vec![[0; 3]; self.state.settings.count];
        if self.controller.status.connected {
            let _ = self.controller.solid([0; 3], 127);
        }
    }
    fn stop(&mut self) {
        self.black();
        self.screen = None;
        self.audio = None;
        self.state.capturing = false;
        self.state.capture_size = (0, 0);
        self.state.audio_samples = 0;
        self.state.audio_bands.fill(0.0);
        self.state.audio_level = 0.0;
        self.state.beat_level = 0.0;
        self.beat = crate::beat::Detector::default();
    }
    fn begin_calibration(&mut self) -> Result<(), String> {
        if self.calibration.is_none() {
            if !self.life.pause_reason(&self.state.settings).is_empty() {
                return Err("Wake and unlock the display before calibrating".into());
            }
            if !self.controller.status.connected {
                self.connect_strip()?;
            }
            self.calibration = Some(CalibrationSession {
                original: self.state.settings.clone(),
                requested: self.life.requested,
                capture_allowed: self.life.capture_allowed,
                started: Instant::now(),
            });
            self.stop();
            self.state.calibrating = true;
            self.state.calibration_led = Some(0);
            self.state.settings.mode = "solid".into();
            self.state.settings.brightness = 20;
            self.life.requested = true;
            self.life.capture_allowed = false;
            self.desired = true;
        }
        Ok(())
    }
    fn step_calibration(&mut self, settings: Settings, led: Option<usize>) -> Result<(), String> {
        if self.calibration.is_none() {
            return Err("Start calibration first".into());
        }
        settings.validate()?;
        if led.is_some_and(|i| i >= settings.count) {
            return Err("LED is outside the selected count".into());
        }
        if settings.count != self.state.settings.count {
            self.black();
        }
        self.state.settings.count = settings.count;
        self.state.settings.sides = settings.sides;
        self.state.settings.reverse = settings.reverse;
        self.state.settings.offset = settings.offset;
        self.state.calibration_led = led;
        Ok(())
    }
    fn end_calibration(&mut self, apply: bool, resume: bool) -> Result<(), String> {
        if let Some(CalibrationSession {
            mut original,
            requested,
            capture_allowed: consent,
            ..
        }) = self.calibration.take()
        {
            if apply {
                let before = original.clone();
                original.count = self.state.settings.count;
                original.sides = self.state.settings.sides;
                original.offset = self.state.settings.offset;
                original.reverse = self.state.settings.reverse;
                if let Err(e) = original.save(&self.path) {
                    self.calibration = Some(CalibrationSession {
                        original: before,
                        requested,
                        capture_allowed: consent,
                        started: Instant::now(),
                    });
                    return Err(e);
                }
            }
            self.stop();
            self.state.settings = original;
            self.state.calibrating = false;
            self.state.calibration_led = None;
            self.life.requested = requested && resume;
            self.life.capture_allowed = consent && resume;
            self.desired = requested && resume;
            self.capture_retry = Instant::now() - Duration::from_secs(3);
            self.remember()?;
        }
        Ok(())
    }
    fn intent_path(&self) -> PathBuf {
        self.path.with_file_name("run-state.json")
    }
    fn remember(&self) -> Result<(), String> {
        SavedIntent {
            running: self.life.requested,
        }
        .save(&self.intent_path())
    }
    fn restore(&mut self) {
        if self.state.handoff_released {
            self.state.status.message =
                "Released for USB handoff. Press Start to take control again.".into();
            return;
        }
        if self.state.settings.mode == "replay" {
            if let Err(error) = self.sequences.validate_playback(&self.state.settings) {
                self.state.error = error;
                return;
            }
        }
        match SavedIntent::load(&self.intent_path()) {
            Ok(intent) if self.state.settings.restore_on_start && intent.running => {
                self.life.requested = true;
                self.life.initializing = true;
                self.life.capture_allowed = self.state.settings.resume_capture;
                self.desired = true;
                // Let native observers deliver initial display/lock state before connecting.
                self.retry = Instant::now();
            }
            Err(e) => self.state.error = e,
            _ => {}
        }
        self.update_lifecycle();
    }
    fn update_lifecycle(&mut self) {
        if self.state.settings.mode == "blend"
            && self.state.capturing
            && self.state.error.is_empty()
        {
            if self.audio.is_none() {
                self.state.error =
                    "Audio unavailable; using screen colors. Click Retry to restore both sources."
                        .into();
            } else if self.screen.is_none() {
                self.state.error =
                    "Screen unavailable; using music. Click Retry to restore both sources.".into();
            }
        }
        self.state.requested = self.life.requested;
        self.state.pause_reason = self.life.pause_reason(&self.state.settings).into();
        self.state.powered = self.life.active(&self.state.settings);
        let capture_mode = matches!(
            self.state.settings.mode.as_str(),
            "screen" | "music" | "blend"
        );
        self.state.capture_requires_start =
            self.life.requested && capture_mode && !self.life.capture_allowed;
        self.state.recovering = self.life.active(&self.state.settings)
            && (!self.controller.status.connected
                || (capture_mode && self.life.capture_allowed && !self.state.capturing));
    }
    fn interrupt_capture(&mut self) {
        self.life.interrupted(&self.state.settings);
        self.stop();
        self.capture_retry = Instant::now();
    }
    fn lose_blend_source(&mut self, screen: bool) {
        if screen {
            self.screen = None;
            self.state.capture_size = (0, 0);
        } else {
            self.audio = None;
            self.state.audio_samples = 0;
            self.state.audio_level = 0.0;
            self.state.audio_bands.fill(0.0);
            self.state.beat_level = 0.0;
            self.beat = crate::beat::Detector::default();
        }
        if self.screen.is_none() && self.audio.is_none() {
            self.interrupt_capture();
            self.state.error = "Both capture sources are unavailable. Click Retry.".into();
        } else {
            self.state.error = if screen {
                "Screen source changed; using music. Click Retry."
            } else {
                "Audio source changed; using screen colors. Click Retry."
            }
            .into();
            self.capture_retry = Instant::now();
        }
    }
    pub fn system_event(&mut self, event: SystemEvent) {
        if matches!(
            event,
            SystemEvent::Sleep(true) | SystemEvent::Locked(true) | SystemEvent::DisplayOff(true)
        ) {
            self.extension = None;
            self.external_cue = None;
        }
        let was_paused = !self.life.pause_reason(&self.state.settings).is_empty();
        match event {
            SystemEvent::Ready => {
                self.life.initializing = false;
                self.state.lifecycle_ready = true;
            }
            SystemEvent::Sleep(sleeping) => {
                if sleeping == self.life.sleeping {
                    return;
                }
                self.life.sleeping = sleeping;
                self.routines.suspend();
                self.fade = None;
                if sleeping {
                    if self.state.settings.off_on_sleep {
                        self.black();
                    }
                    // Close USB before stream teardown, which may need the OS GUI thread.
                    self.controller.close("Computer sleeping");
                }
            }
            SystemEvent::DisplayOff(off) => self.life.display_off = off,
            SystemEvent::Locked(locked) => self.life.locked = locked,
            SystemEvent::SourcesChanged => {
                if self.state.capturing && self.state.settings.mode == "blend" {
                    self.lose_blend_source(true);
                } else if self.state.capturing && self.state.settings.mode == "screen" {
                    self.interrupt_capture();
                }
            }
            #[cfg(target_os = "linux")]
            SystemEvent::AudioChanged => {
                if self.state.capturing && self.state.settings.mode == "blend" {
                    self.lose_blend_source(false);
                } else if self.state.capturing && self.state.settings.mode == "music" {
                    self.interrupt_capture();
                }
            }
            SystemEvent::Warning(message) => {
                if !self.state.lifecycle_warning.contains(&message) {
                    if !self.state.lifecycle_warning.is_empty() {
                        self.state.lifecycle_warning.push(' ');
                    }
                    self.state.lifecycle_warning.push_str(&message);
                }
            }
        }
        let paused = !self.life.pause_reason(&self.state.settings).is_empty();
        if paused && !was_paused {
            if let Err(e) = self.end_calibration(false, true) {
                self.state.error = e;
            }
            self.interrupt_capture();
        }
        if was_paused && !paused {
            self.retry = Instant::now() - Duration::from_secs(3);
            self.capture_retry = Instant::now() - Duration::from_secs(3);
            self.last_solid = None;
        }
        self.update_lifecycle();
        self.state.status = self.controller.status.clone();
    }
    fn begin_capture(&mut self, explicit: bool) -> Result<(), String> {
        self.stop();
        if !explicit {
            crate::platform::check_capture_permission(&self.state.settings)?;
        }
        self.state.error.clear();
        match self.state.settings.mode.as_str() {
            "blend" => {
                let screen = ScreenCapture::start(&self.state.settings);
                let audio = Audio::start(self.state.settings.audio_source == "system");
                let errors: Vec<_> = [screen.as_ref().err(), audio.as_ref().err()]
                    .into_iter()
                    .flatten()
                    .cloned()
                    .collect();
                self.screen = screen.ok();
                self.audio = audio.ok();
                if self.screen.is_none() && self.audio.is_none() {
                    return Err(errors.join(" "));
                }
                if !errors.is_empty() {
                    self.state.error = format!(
                        "Blend using available source. {} Click Retry to restore both.",
                        errors.join(" ")
                    );
                }
            }
            "screen" => self.screen = Some(ScreenCapture::start(&self.state.settings)?),
            "music" => {
                self.audio = Some(Audio::start(self.state.settings.audio_source == "system")?)
            }
            _ => return Err("Select Screen or Music before starting capture".into()),
        }
        self.state.capturing = true;
        if self.state.settings.mode == "blend" {
            self.blend_sources = (
                crate::capture::source_signature(&self.state.settings).ok(),
                crate::audio::source_signature(self.state.settings.audio_source == "system").ok(),
            );
        }
        self.source_signature = self.current_source_signature().ok();
        self.source_check = Instant::now();
        Ok(())
    }
    fn current_source_signature(&self) -> Result<String, String> {
        match self.state.settings.mode.as_str() {
            "blend" => Ok(format!(
                "{:?}|{:?}",
                crate::capture::source_signature(&self.state.settings),
                crate::audio::source_signature(self.state.settings.audio_source == "system")
            )),
            "screen" => crate::capture::source_signature(&self.state.settings),
            "music" => crate::audio::source_signature(self.state.settings.audio_source == "system"),
            _ => Ok(String::new()),
        }
    }
    fn start_requested(&mut self) -> Result<(), String> {
        if self.state.settings.mode == "replay" {
            self.sequences.validate_playback(&self.state.settings)?;
        }
        self.started = Instant::now();
        self.life.requested = true;
        self.life.capture_allowed = true;
        self.desired = true;
        self.remember()?;
        self.update_lifecycle();
        if !self.life.active(&self.state.settings) {
            return Ok(());
        }
        self.state.error.clear();
        self.retry = Instant::now();
        if !self.controller.status.connected {
            self.connect_strip()?;
        }
        self.last_solid = None;
        if matches!(
            self.state.settings.mode.as_str(),
            "screen" | "music" | "blend"
        ) {
            if let Err(e) = self.begin_capture(true) {
                // An initial denial requires another deliberate Start, never a prompt loop.
                self.life.capture_allowed = false;
                self.update_lifecycle();
                return Err(e);
            }
        }
        Ok(())
    }
    fn apply_settings(&mut self, mut settings: Settings, persist: bool) -> Result<(), String> {
        settings.mode_settings = self.state.settings.mode_settings.clone();
        // Do not create memory on a first same-mode edit; older settings remain valid.
        if settings.mode != self.state.settings.mode {
            settings.mode_settings.insert(
                self.state.settings.mode.clone(),
                crate::settings::ModeSettings::capture(&self.state.settings),
            );
        }
        if settings.mode_settings.contains_key(&settings.mode) {
            settings.mode_settings.insert(
                settings.mode.clone(),
                crate::settings::ModeSettings::capture(&settings),
            );
        }
        settings.validate()?;
        if settings.mode == "replay" {
            self.sequences.validate_playback(&settings)?;
        }
        if settings.device_id != self.state.settings.device_id {
            self.action(Action::Power { on: false })?;
        }
        if persist {
            settings.save(&self.path)?;
        }
        self.controller.bind(settings.device_id.clone());
        if settings.mode != self.state.settings.mode
            || settings.audio_source != self.state.settings.audio_source
            || settings.screen_id != self.state.settings.screen_id
            || settings.capture_source != self.state.settings.capture_source
        {
            self.stop();
            self.life.capture_allowed = false;
        }
        if settings.count != self.state.settings.count {
            self.black();
        }
        let was_paused = !self.life.pause_reason(&self.state.settings).is_empty();
        if self.state.settings.resume_capture && !settings.resume_capture && !self.state.capturing {
            self.life.capture_allowed = false;
        }
        if settings.mode != self.state.settings.mode
            || settings.effect != self.state.settings.effect
            || settings.studio != self.state.settings.studio
            || settings.replay != self.state.settings.replay
        {
            self.started = Instant::now();
        }
        self.state.settings = settings;
        if !was_paused && !self.life.pause_reason(&self.state.settings).is_empty() {
            self.interrupt_capture();
        }
        self.last_solid = None;
        self.state.error.clear();
        Ok(())
    }
    pub fn action(&mut self, action: Action) -> Result<Snapshot, String> {
        let manual_brightness = match &action {
            Action::Settings { settings } | Action::PreviewSettings { settings } => {
                settings.brightness != self.state.settings.brightness
            }
            Action::PatchSettings { patch, .. } => patch
                .0
                .get("brightness")
                .is_some_and(|v| v.as_u64() != Some(u64::from(self.state.settings.brightness))),
            Action::Adjust {
                brightness: Some(_),
                ..
            }
            | Action::PreviewBrightness { .. } => true,
            _ => false,
        };
        if manual_brightness {
            self.ambient.manual();
        }
        // A manual change atomically invalidates any pending app-rule restore/fade.
        // Internal routine changes call apply_settings directly on this worker.
        let changed = match &action {
            Action::Settings { settings } | Action::PreviewSettings { settings } => {
                settings != &self.state.settings
            }
            Action::PatchSettings { patch, .. } => patch
                .apply(&self.state.settings)
                .is_ok_and(|s| s != self.state.settings),
            _ => true,
        };
        if changed
            && matches!(
                action,
                Action::Start
                    | Action::StartMode { .. }
                    | Action::SelectMode { .. }
                    | Action::PreviewBrightness { .. }
                    | Action::ApplyProfile { .. }
                    | Action::Adjust { .. }
                    | Action::Toggle
                    | Action::CalibrationStart
                    | Action::Disconnect
                    | Action::PatchSettings { .. }
                    | Action::Settings { .. }
                    | Action::PreviewSettings { .. }
                    | Action::Power { .. }
                    | Action::StartCapture
                    | Action::StopCapture
                    | Action::PrepareUpdate
                    | Action::Shutdown
            )
        {
            self.extension = None;
            self.external_cue = None;
            self.routines.manual_override();
            self.routine_restore = None;
            self.fade = None;
        }
        if self.calibration.is_some()
            && matches!(
                action,
                Action::Start
                    | Action::StartMode { .. }
                    | Action::SelectMode { .. }
                    | Action::Settings { .. }
                    | Action::PreviewSettings { .. }
                    | Action::PatchSettings { .. }
            )
        {
            return Err("Finish or cancel strip calibration first".into());
        }
        let persist = matches!(action, Action::Settings { .. });
        match action {
            Action::Extension { overlay, cue } => {
                if !overlay.live() {
                    return Err("Extension has been revoked".into());
                }
                if !self.state.requested
                    || self.state.calibrating
                    || !self.state.pause_reason.is_empty()
                {
                    return Err("Start ordinary lighting and finish calibration before activating extensions".into());
                }
                if cue {
                    self.external_cue = Some(overlay);
                } else {
                    self.extension = Some(overlay);
                }
            }
            Action::AmbientConfig { config } => {
                if let Some(config) = config {
                    self.ambient
                        .configure(config, self.state.settings.brightness)?;
                }
            }
            Action::AmbientReading { source, result } => self.ambient.reading(&source, result),
            Action::AmbientResume => self.ambient.resume(self.state.settings.brightness),
            Action::ExportHandoff { path } => {
                if self.state.calibrating {
                    return Err("Finish calibration before handoff".into());
                }
                crate::settings::save_json(
                    &crate::handoff::Profile::capture(&self.state.settings),
                    &path,
                )?;
                let mut config = self.routines.config.clone();
                config.paused = true;
                config.apps_enabled = false;
                self.routines.configure(config)?;
                crate::settings::save_json(
                    &true,
                    &self.path.with_file_name("handoff-released.json"),
                )?;
                self.state.handoff_released = true;
                let stopped = self.action(Action::Power { on: false });
                self.controller.close(
                    "Released for USB handoff. Switch/unplug USB; Start takes control again.",
                );
                stopped?;
            }
            Action::ImportHandoff { profile } => {
                if self.state.calibrating {
                    return Err("Finish calibration before handoff import".into());
                }
                profile.validate()?;
                let source = SourceBinding {
                    screen_id: self.state.settings.screen_id.clone(),
                    capture_source: self.state.settings.capture_source.clone(),
                };
                let mut settings =
                    crate::profiles::scene(&profile.scene, &source, &self.state.settings);
                settings.count = profile.count;
                settings.sides = profile.sides;
                settings.offset = profile.offset;
                settings.reverse = profile.reverse;
                settings.validate()?;
                if settings.mode == "replay" {
                    self.sequences.validate_playback(&settings)?;
                }
                self.action(Action::Power { on: false })?;
                self.apply_settings(settings, true)?;
                self.routines.status.scene_revision += 1;
            }
            Action::BindController { id } => {
                if self.state.calibrating {
                    return Err("Finish calibration before rebinding".into());
                }
                let settings = Settings {
                    device_id: id.clone(),
                    ..self.state.settings.clone()
                };
                settings.validate()?;
                self.action(Action::Power { on: false })?;
                self.controller.bind(id);
                self.apply_settings(settings, true)?;
            }
            Action::GroupBrightness { value } => {
                if self.state.calibrating {
                    return Err("Finish calibration before grouped brightness changes".into());
                }
                if value > 100 {
                    return Err("Group brightness must be 0–100".into());
                }
                crate::settings::save_json(&value, &self.path.with_file_name("group-level.json"))?;
                self.state.group_brightness = value;
                self.last_solid = None;
            }
            Action::RoutinesConfig { config } => {
                if let Some(config) = config {
                    config.validate()?;
                    self.restore_routine()?;
                    self.routines.configure(config)?;
                    self.fade = None;
                }
            }
            Action::Routine { command } => {
                self.routines
                    .command(&command, self.state.powered && !self.state.calibrating)?;
            }
            Action::Foreground { result } => {
                if self.routines.config.apps_enabled {
                    match result {
                        Err(error) => {
                            self.restore_routine()?;
                            self.routines.foreground.clear();
                            self.routines.status.message =
                                format!("Application rules unavailable: {error}");
                        }
                        Ok(app) => {
                            if app != self.routines.foreground {
                                let establish_baseline = self.routines.app_override
                                    && self.routines.foreground.is_empty();
                                self.restore_routine()?;
                                self.routines.foreground = app.clone();
                                self.routines.app_override = false;
                                if !establish_baseline
                                    && self.life.requested
                                    && !self.state.calibrating
                                    && self.life.pause_reason(&self.state.settings).is_empty()
                                {
                                    if let Some(rule) = self
                                        .routines
                                        .config
                                        .app_rules
                                        .iter()
                                        .find(|r| r.app == app)
                                        .cloned()
                                    {
                                        self.routine_restore = Some((
                                            SceneSettings::capture(&self.state.settings),
                                            SourceBinding {
                                                screen_id: self.state.settings.screen_id.clone(),
                                                capture_source: self
                                                    .state
                                                    .settings
                                                    .capture_source
                                                    .clone(),
                                            },
                                            self.life.requested,
                                        ));
                                        if let Err(e) = self.apply_routine(&rule.scene) {
                                            self.restore_routine()?;
                                            self.routines.status.message =
                                                format!("Application rule skipped: {e}");
                                        } else {
                                            self.routines.status.active_rule =
                                                format!("{} → {}", app, rule.scene.name);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Action::Remote { action, active } => {
                if !self.state.pause_reason.is_empty()
                    && !matches!(*action, Action::Power { on: false })
                {
                    return Err("Host lighting is paused".into());
                }
                return self.action(Action::Automation {
                    action,
                    active,
                    allow_capture: false,
                });
            }
            Action::Automation {
                action,
                active,
                allow_capture,
            } => {
                // Check consent on the worker, against the exact settings this command
                // will use. A renderer edit cannot race a preflight snapshot.
                if !active.load(std::sync::atomic::Ordering::Acquire) {
                    return Err("Automation session revoked".into());
                }
                if self.calibration.is_some()
                    && !matches!(
                        *action,
                        Action::Power { on: false } | Action::Routine { .. }
                    )
                {
                    return Err("Finish or cancel strip calibration first".into());
                }
                let target = match action.as_ref() {
                    Action::Start => Some(self.state.settings.clone()),
                    Action::Toggle if !self.life.requested => Some(self.state.settings.clone()),
                    Action::ApplyProfile { scene, source } => {
                        Some(crate::profiles::scene(scene, source, &self.state.settings))
                    }
                    Action::Toggle
                    | Action::Routine { .. }
                    | Action::Extension { .. }
                    | Action::Adjust {
                        brightness: None,
                        color: Some(_),
                    }
                    | Action::Power { on: false }
                    | Action::Adjust {
                        brightness: Some(_),
                        color: None,
                    } => None,
                    _ => return Err("Unsupported automation action".into()),
                };
                if let Some(target) = target {
                    if matches!(target.mode.as_str(), "screen" | "music" | "blend") {
                        if !allow_capture {
                            return Err("Capture requires permission in Settings and --allow-capture on this command".into());
                        }
                        // Remote commands cannot trigger a new OS permission prompt.
                        crate::platform::check_capture_permission(&target)?;
                    }
                }
                return self.action(*action);
            }
            Action::RecordStart { seconds } => {
                if self.state.calibrating {
                    return Err("Finish calibration before recording".into());
                }
                self.sequences.begin(&self.state.settings, seconds)?;
            }
            Action::RecordStop {} => self.sequences.finish()?,
            Action::LoadSequence { sequence } => {
                if self.state.settings.mode == "replay" {
                    self.action(Action::Power { on: false })?;
                }
                self.sequences.replace(sequence)?;
            }
            Action::ExportSequence { path } => self.sequences.export(&path)?,
            Action::Snapshot => {}
            Action::PatchSettings { patch, persist } => {
                let settings = patch.apply(&self.state.settings)?;
                self.apply_settings(settings, persist)?;
            }
            Action::PreviewBrightness { brightness } => {
                let patch = SettingsPatch(serde_json::Map::from_iter([(
                    "brightness".into(),
                    brightness.into(),
                )]));
                return self.action(Action::PatchSettings {
                    patch,
                    persist: false,
                });
            }
            Action::SelectMode { mode } => {
                let mut settings = self.state.settings.clone();
                if let Some(memory) = settings.mode_settings.get(&mode).cloned() {
                    memory.apply(&mut settings);
                }
                settings.mode = mode;
                return self.action(Action::Settings { settings });
            }
            Action::StartMode { mode } => {
                // Restore per-mode settings and start in one worker message.
                self.action(Action::SelectMode { mode })?;
                return self.action(Action::Start);
            }
            Action::ApplyProfile { scene, source } => {
                let settings = crate::profiles::scene(&scene, &source, &self.state.settings);
                self.action(Action::Settings { settings })?;
                return self.action(Action::Start);
            }
            Action::Adjust { brightness, color } => {
                let mut patch = SettingsPatch::default();
                if let Some(brightness) = brightness {
                    patch.0.insert("brightness".into(), brightness.into());
                }
                if let Some(color) = color {
                    patch.0.insert("mode".into(), "solid".into());
                    patch.0.insert("color".into(), color.into());
                }
                return self.action(Action::PatchSettings {
                    patch,
                    persist: true,
                });
            }
            Action::Toggle => {
                return self.action(if self.life.requested {
                    Action::Power { on: false }
                } else {
                    Action::Start
                })
            }
            Action::CalibrationStart => {
                if self.sequences.status().recording {
                    return Err("Stop recording before calibration".into());
                }
                self.begin_calibration()?;
            }
            Action::CalibrationStep { settings, led } => {
                self.step_calibration(settings, led)?;
            }
            Action::CalibrationEnd { apply } => self.end_calibration(apply, true)?,
            Action::Start => {
                if self.state.handoff_released {
                    crate::settings::save_json(
                        &false,
                        &self.path.with_file_name("handoff-released.json"),
                    )?;
                    self.state.handoff_released = false;
                }
                if let Err(e) = self.start_requested() {
                    self.state.error = e;
                }
            }
            Action::Connect => {
                if self.state.handoff_released {
                    return Err(
                        "Released for handoff. Press Start to take control explicitly.".into(),
                    );
                }
                self.state.error.clear();
                self.desired = true;
                self.retry = Instant::now();
                self.connect_strip()?;
                self.last_solid = None;
            }
            Action::Disconnect => {
                self.end_calibration(false, false)?;
                self.desired = false;
                self.life.stop();
                self.stop();
                self.controller.close("Disconnected");
                self.remember()?;
            }
            Action::Settings { settings } | Action::PreviewSettings { settings } => {
                self.apply_settings(settings, persist)?;
            }
            Action::Power { on } => {
                if on && self.state.handoff_released {
                    return Err(
                        "Released for handoff. Press Start to take control explicitly.".into(),
                    );
                }
                if !on {
                    self.end_calibration(false, false)?;
                }
                if !self.life.pause_reason(&self.state.settings).is_empty() && on {
                    return Err("Computer is sleeping".into());
                }
                self.state.powered = on;
                self.life.requested = on;
                self.last_solid = None;
                self.state.error.clear();
                if !on {
                    self.life.stop();
                    self.desired = false;
                    self.stop();
                }
                self.remember()?;
            }
            Action::StartCapture => {
                if !self.life.active(&self.state.settings) {
                    return Err("Turn lights on before starting capture".into());
                }
                self.life.capture_allowed = false;
                self.begin_capture(true)?;
                self.life.capture_allowed = true;
            }
            Action::StopCapture => {
                self.end_calibration(false, false)?;
                self.life.stop();
                self.desired = false;
                self.stop();
                self.remember()?;
            }
            Action::PrepareUpdate | Action::Shutdown => {
                if self.sequences.status().recording {
                    if let Err(e) = self.sequences.finish() {
                        self.state.error = e;
                    }
                }
                self.end_calibration(false, true)?;
                if matches!(action, Action::PrepareUpdate) {
                    self.state.settings.save(&self.path)?;
                }
                self.life.stop();
                self.desired = false;
                if self.state.settings.off_on_quit {
                    self.black();
                }
                self.controller.close("Closed");
                self.screen = None;
                self.audio = None;
                self.state.capturing = false;
            }
        }
        self.update_lifecycle();
        self.state.status = self.controller.status.clone();
        self.state.sequence = self.sequences.status();
        self.sync_routines();
        Ok(self.state.clone())
    }
    fn sync_routines(&mut self) {
        self.state.ambient = self.ambient.status.clone();
        self.routines.status.paused = self.routines.config.paused;
        self.routines.status.watch_apps =
            self.routines.config.apps_enabled && !self.routines.config.app_rules.is_empty();
        self.state.routines = self.routines.status.clone();
        self.state.routines_config = self.routines.config.clone();
    }
    fn apply_routine(&mut self, scene: &crate::routines::Scene) -> Result<(), String> {
        let settings = crate::profiles::scene(&scene.settings, &scene.source, &self.state.settings);
        if matches!(settings.mode.as_str(), "screen" | "music" | "blend") {
            return Err("Start capture manually".into());
        }
        self.apply_settings(settings, false)?;
        self.routines.status.scene_revision += 1;
        self.start_requested()
    }
    fn restore_routine(&mut self) -> Result<(), String> {
        self.routines.status.active_rule.clear();
        if let Some((scene, source, requested)) = self.routine_restore.take() {
            let settings = crate::profiles::scene(&scene, &source, &self.state.settings);
            self.apply_settings(settings, false)?;
            self.routines.status.scene_revision += 1;
            if requested && !matches!(scene.mode.as_str(), "screen" | "music" | "blend") {
                if self.life.pause_reason(&self.state.settings).is_empty() {
                    self.start_requested()?;
                } else {
                    self.life.requested = true;
                    self.desired = true;
                }
            } else {
                self.life.stop();
                self.desired = false;
                self.stop();
                self.remember()?;
                if requested {
                    self.routines.status.message = "Previous capture scene restored. Press Start to grant fresh capture consent.".into();
                }
            }
        }
        Ok(())
    }
    fn tick_routines(&mut self) {
        let available = !self.state.handoff_released
            && !self.state.calibrating
            && self.life.pause_reason(&self.state.settings).is_empty();
        self.routines.tick_timer(
            Instant::now(),
            available,
            self.state.powered,
            crate::routines::local_minute(),
        );
        if let Some(schedule) = self.routines.schedule(chrono::Local::now(), available) {
            self.routine_restore = None;
            self.routines.manual_override();
            self.fade = None;
            let result = if matches!(schedule.transition, crate::routines::Transition::FadeOut) {
                self.fade = self.life.requested.then(|| {
                    (
                        Instant::now(),
                        Duration::from_secs(schedule.seconds.into()),
                        true,
                    )
                });
                Ok(())
            } else {
                self.apply_routine(&schedule.scene).map(|()| {
                    if matches!(schedule.transition, crate::routines::Transition::FadeIn) {
                        self.fade = Some((
                            Instant::now(),
                            Duration::from_secs(schedule.seconds.into()),
                            false,
                        ));
                    }
                })
            };
            self.routines.status.message = match result {
                Ok(()) => format!("Scheduled action: {}", schedule.scene.name),
                Err(e) => format!("Schedule skipped: {e}"),
            };
        }
        if self
            .fade
            .is_some_and(|(start, length, _)| start.elapsed() >= length)
        {
            let (_, _, off) = self.fade.take().unwrap();
            if off {
                let _ = self.action(Action::Power { on: false });
            }
        }
        self.sync_routines();
    }
    fn reconcile_capture(&mut self) {
        if self.state.capturing && self.source_check.elapsed() >= Duration::from_secs(2) {
            self.source_check = Instant::now();
            if self.state.settings.mode == "blend" {
                let sources = (
                    crate::capture::source_signature(&self.state.settings).ok(),
                    crate::audio::source_signature(self.state.settings.audio_source == "system")
                        .ok(),
                );
                if self.screen.is_some() && sources.0 != self.blend_sources.0 {
                    self.lose_blend_source(true);
                }
                if self.audio.is_some() && sources.1 != self.blend_sources.1 {
                    self.lose_blend_source(false);
                }
                self.blend_sources = sources;
            } else {
                let signature = self.current_source_signature();
                if signature.as_ref().ok() != self.source_signature.as_ref() {
                    if matches!(self.state.settings.mode.as_str(), "screen" | "blend") {
                        self.system_event(SystemEvent::SourcesChanged);
                    } else {
                        self.interrupt_capture();
                    }
                    self.state.error = "Capture source changed. Waiting to reconnect.".into();
                }
            }
        }
        if self.state.settings.mode == "blend"
            && self.state.capturing
            && self.life.active(&self.state.settings)
            && self.state.settings.resume_capture
            && self.life.capture_allowed
            && (self.screen.is_none() || self.audio.is_none())
            && self.capture_retry.elapsed() >= Duration::from_secs(3)
        {
            self.capture_retry = Instant::now();
            if crate::platform::check_capture_permission(&self.state.settings).is_ok() {
                if self.screen.is_none() {
                    self.screen = ScreenCapture::start(&self.state.settings).ok();
                }
                if self.audio.is_none() {
                    self.audio = Audio::start(self.state.settings.audio_source == "system").ok();
                }
                self.blend_sources = (
                    crate::capture::source_signature(&self.state.settings).ok(),
                    crate::audio::source_signature(self.state.settings.audio_source == "system")
                        .ok(),
                );
                if self.screen.is_some() && self.audio.is_some() {
                    self.state.error.clear();
                }
            }
        }
        if self.life.active(&self.state.settings)
            && self.controller.status.connected
            && self.life.capture_allowed
            && !self.state.capturing
            && matches!(
                self.state.settings.mode.as_str(),
                "screen" | "music" | "blend"
            )
            && self.capture_retry.elapsed() >= Duration::from_secs(3)
        {
            self.capture_retry = Instant::now();
            if let Err(e) = self.begin_capture(false) {
                self.state.error = format!("Waiting for capture: {e}");
            }
        }
    }
    fn generate_frame(&mut self, dt: f32) -> Result<Vec<Rgb>, String> {
        let s = &self.state.settings;

        if !self.life.active(s) {
            return Ok(vec![[0; 3]; s.count]);
        }
        if self.state.calibrating {
            return Ok((0..s.count)
                .map(|i| match self.state.calibration_led {
                    Some(led) if led == i => [255; 3],
                    Some(_) => [0; 3],
                    None => {
                        let (x, y) = lighting::layout(s, 16.0 / 9.0)[i];
                        if y == 0.0 {
                            [0, 255, 0]
                        } else if x == 0.0 {
                            [255, 0, 0]
                        } else if x == 1.0 {
                            [0, 80, 255]
                        } else {
                            [255, 150, 0]
                        }
                    }
                })
                .collect());
        }
        match s.mode.as_str() {
            "replay" => self.sequences.sample(s, self.started.elapsed()),
            "solid" => Ok(vec![
                if s.studio.bias.enabled {
                    s.studio.bias.color()
                } else {
                    lighting::hex(&s.color)
                };
                s.count
            ]),
            "rainbow" => Ok(effects::animated(s, self.started.elapsed().as_secs_f32())),
            "screen" => {
                if let Some(screen) = self.screen.as_mut() {
                    let colors = screen.colors(s)?;
                    self.state.capture_size = screen.dimensions;
                    Ok(colors)
                } else {
                    Ok(vec![[0; 3]; s.count])
                }
            }
            "music" | "blend" => {
                if let Some(audio) = self.audio.as_ref() {
                    let analysis = match audio.analyze(s.sensitivity, s.noise_floor, s.input_gain) {
                        Ok(a) => a,
                        Err(e) if s.mode == "blend" && self.screen.is_some() => {
                            self.audio = None;
                            self.state.audio_bands.fill(0.0);
                            self.state.audio_level = 0.0;
                            self.state.audio_samples = 0;
                            self.state.beat_level = 0.0;
                            self.state.error =
                                format!("Audio unavailable; using screen colors. {e} Click Retry.");
                            return self.screen.as_mut().unwrap().colors(s);
                        }
                        Err(e) => return Err(e),
                    };
                    self.state.beat_level = self.beat.update(&analysis.bands, dt, s.sensitivity);
                    self.state.audio_samples = analysis.received;
                    self.state.audio_level = analysis.rms;
                    effects::envelope(&mut self.state.audio_bands, &analysis.bands, dt, s.release);
                    if s.mode == "blend" {
                        if let Some(screen) = self.screen.as_mut() {
                            match screen.colors(s) {
                                Ok(colors) => {
                                    self.state.capture_size = screen.dimensions;
                                    return Ok(effects::blend(
                                        &colors,
                                        &self.state.audio_bands,
                                        s.blend,
                                    ));
                                }
                                Err(e) => {
                                    self.state.error = format!(
                                        "Screen unavailable; using music. {e} Click Retry."
                                    );
                                    self.screen = None;
                                    self.state.capture_size = (0, 0);
                                }
                            }
                        }
                    }
                    Ok(if s.music_style == "beat" {
                        effects::music(
                            s,
                            &[self.state.beat_level; 32],
                            self.started.elapsed().as_secs_f32(),
                        )
                    } else {
                        effects::music(
                            s,
                            &self.state.audio_bands,
                            self.started.elapsed().as_secs_f32(),
                        )
                    })
                } else if s.mode == "blend" {
                    if let Some(screen) = self.screen.as_mut() {
                        let colors = screen.colors(s)?;
                        self.state.capture_size = screen.dimensions;
                        Ok(colors)
                    } else {
                        Ok(vec![[0; 3]; s.count])
                    }
                } else {
                    Ok(vec![[0; 3]; s.count])
                }
            }
            _ => Ok(vec![[0; 3]; s.count]),
        }
    }
    pub fn tick(&mut self) {
        if !self.state.requested || self.state.calibrating || !self.state.pause_reason.is_empty() {
            self.extension = None;
            self.external_cue = None;
        }
        if self.extension.as_ref().is_some_and(|o| !o.live()) {
            self.extension = None;
        }
        if self.external_cue.as_ref().is_some_and(|o| !o.live()) {
            self.external_cue = None;
        }
        if self
            .calibration
            .as_ref()
            .is_some_and(|session| session.started.elapsed() > Duration::from_secs(300))
        {
            let _ = self.end_calibration(false, false);
            self.state.error = "Calibration timed out. Lighting stopped.".into();
        }
        self.update_lifecycle();
        let dt = self.last_tick.elapsed().as_secs_f32().min(1.0);
        self.last_tick = Instant::now();
        self.tick_routines();
        if self.desired
            && self.life.pause_reason(&self.state.settings).is_empty()
            && !self.controller.status.connected
            && self.retry.elapsed() >= Duration::from_secs(3)
        {
            self.retry = Instant::now();
            match self.connect_strip() {
                Ok(()) => self.state.error.clear(),
                Err(e) => self.state.error = format!("Waiting for strip: {e}"),
            }
            self.last_solid = None;
        }
        self.reconcile_capture();
        let generated = self.generate_frame(dt);
        match generated {
            Ok(mut colors) => {
                if !self.state.calibrating && self.state.settings.mode != "replay" {
                    self.state.settings.studio.overlay(
                        &mut colors,
                        self.started.elapsed().as_secs_f32(),
                        self.state.settings.speed,
                        self.state.settings.effect_reverse,
                    );
                }
                if !self.state.powered
                    || (matches!(
                        self.state.settings.mode.as_str(),
                        "screen" | "music" | "blend"
                    ) && !self.state.capturing)
                {
                    self.previous = vec![[0.0; 3]; self.state.settings.count];
                } else if self.state.calibrating {
                    self.previous = colors
                        .iter()
                        .map(|rgb| {
                            rgb.map(|c| {
                                c as f32
                                    * self
                                        .state
                                        .settings
                                        .brightness
                                        .min(self.state.settings.max_brightness)
                                        as f32
                                    / 100.0
                            })
                        })
                        .collect();
                } else {
                    if let Some(effect) = &self.extension {
                        effect.render(
                            &mut colors,
                            self.state.settings.speed,
                            self.state.settings.effect_reverse,
                        );
                    }
                    use chrono::Timelike;
                    let now = chrono::Local::now();
                    if !self
                        .routines
                        .config
                        .quiet((now.hour() * 60 + now.minute()) as u16)
                    {
                        if let Some(cue) = &self.external_cue {
                            cue.render(
                                &mut colors,
                                self.state.settings.speed,
                                self.state.settings.effect_reverse,
                            );
                        }
                    } else {
                        self.external_cue = None;
                    }
                    self.routines.overlay(&mut colors);
                    let mut output_settings = self.state.settings.clone();
                    output_settings.brightness =
                        self.ambient.brightness(output_settings.brightness, dt);
                    output_settings.brightness = (u16::from(output_settings.brightness)
                        * u16::from(self.state.group_brightness)
                        / 100) as u8;
                    output_settings.minimum_glow = (u16::from(output_settings.minimum_glow)
                        * u16::from(self.state.group_brightness)
                        / 100) as u8;
                    if let Some((start, length, off)) = self.fade {
                        let fraction =
                            (start.elapsed().as_secs_f32() / length.as_secs_f32()).clamp(0.0, 1.0);
                        let gain = if off { 1.0 - fraction } else { fraction };
                        output_settings.brightness =
                            (f32::from(output_settings.brightness) * gain).round() as u8;
                        output_settings.minimum_glow =
                            (f32::from(output_settings.minimum_glow) * gain).round() as u8;
                    }
                    self.previous =
                        lighting::transform(&colors, &self.previous, &output_settings, dt);
                }
                self.state.colors = self
                    .previous
                    .iter()
                    .map(|rgb| rgb.map(|c| c.round().clamp(0.0, 255.0) as u8))
                    .collect();
            }
            Err(e) => {
                self.interrupt_capture();
                self.previous = vec![[0.0; 3]; self.state.settings.count];
                self.state.colors = vec![[0; 3]; self.state.settings.count];
                self.state.error = format!("Lighting source unavailable: {e}");
            }
        }
        if let Err(e) = self
            .sequences
            .record(&self.state.colors, &self.state.settings)
        {
            self.state.error = e;
        }
        self.state.sequence = self.sequences.status();
        if self.controller.status.connected && !self.life.sleeping {
            let write_started = Instant::now();
            let s = &self.state.settings;
            let active = self.state.powered
                && (self.state.calibrating
                    || matches!(s.mode.as_str(), "rainbow" | "replay")
                    || self.state.capturing
                    || self.extension.is_some()
                    || self.external_cue.is_some()
                    || self.state.colors.windows(2).any(|pair| pair[0] != pair[1])
                    || (s.mode == "solid" && !s.studio.zones.is_empty()));
            let result = if active {
                // A frame changes device output independently of the cached solid.
                // Invalidate it so removing a zone/extension restores immediately.
                self.last_solid = None;
                Some(self.controller.frame(&lighting::reorder(
                    &self.state.colors,
                    s.reverse && !(self.state.calibrating && self.state.calibration_led.is_some()),
                    if self.state.calibrating && self.state.calibration_led.is_some() {
                        0
                    } else {
                        s.offset
                    },
                )))
            } else {
                let rgb = if self.state.powered && s.mode == "solid" {
                    self.state.colors.first().copied().unwrap_or([0; 3])
                } else {
                    [0; 3]
                };
                let signature = (rgb, s.count);
                if self.last_solid != Some(signature)
                    || self.heartbeat.elapsed() > Duration::from_secs(2)
                {
                    self.last_solid = Some(signature);
                    self.heartbeat = Instant::now();
                    Some(self.controller.solid(rgb, s.count))
                } else {
                    None
                }
            };
            if let Some(Ok(())) = result {
                if self.state.error.starts_with("USB write failed:") {
                    self.state.error.clear();
                }
                self.fps_frames += 1;
                self.state.frames_written += 1;
                self.last_write = Some(Instant::now());
                self.state.write_duration_ms = write_started.elapsed().as_secs_f64() * 1000.0;
            } else if let Some(Err(e)) = result {
                self.interrupt_capture();
                self.state.write_errors += 1;
                self.state.error = format!("USB write failed: {e}");
            }
        }
        self.update_lifecycle();
        self.state.last_write_ms = self.last_write.map(|t| t.elapsed().as_millis() as u64);
        let elapsed = self.fps_start.elapsed();
        if elapsed >= Duration::from_secs(1) {
            self.state.fps = (self.fps_frames as f64 / elapsed.as_secs_f64()).round() as u32;
            self.fps_frames = 0;
            self.fps_start = Instant::now();
        }
        self.state.status = self.controller.status.clone();
    }
}
pub fn start(
    settings: Settings,
    path: PathBuf,
    warning: String,
    emit: impl Fn(&Snapshot) + Send + 'static,
) -> Handle {
    let (tx, rx): (SyncSender<Message>, Receiver<Message>) = mpsc::sync_channel(32);
    let (events, event_rx) = mpsc::channel::<(SystemEvent, SyncSender<()>)>();
    thread::Builder::new()
        .name("lighting".into())
        .spawn(move || {
            // WASAPI otherwise initializes this thread as STA first, making
            // later WinRT/MTA window capture fail with RPC_E_CHANGED_MODE.
            // Keep COM alive across every screen/audio start/stop, and drop it
            // after the engine and its capture/audio objects have been dropped.
            #[cfg(target_os = "windows")]
            let _apartment = crate::capture_windows::Apartment::new()
                .expect("initialize lighting worker Windows Runtime");
            let mut engine = Engine::new(settings, path, warning);
            engine.restore();
            let mut clock = crate::platform::FrameClock::new();
            let mut next = Instant::now();
            let mut last_event = Instant::now();
            loop {
                while let Ok((event, reply)) = event_rx.try_recv() {
                    engine.system_event(event);
                    // Time spent suspended is not a dropped animation frame.
                    next = Instant::now();
                    let _ = reply.send(());
                }
                match rx.recv_timeout(next.saturating_duration_since(Instant::now())) {
                    Ok(msg) => {
                        let quitting = matches!(msg.action, Action::Shutdown);
                        let result = engine.action(msg.action);
                        if let Err(ref e) = result {
                            engine.state.error = e.clone();
                            engine.state.status = engine.controller.status.clone();
                        }
                        emit(&engine.state);
                        let _ = msg.reply.send(result);
                        if quitting {
                            break;
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        let _ = engine.action(Action::Shutdown);
                        break;
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }
                if Instant::now() >= next {
                    clock.set_active(
                        !engine.life.sleeping
                            && engine.state.powered
                            && (engine.state.capturing
                                || matches!(
                                    engine.state.settings.mode.as_str(),
                                    "rainbow" | "replay"
                                )
                                || engine.extension.is_some()
                                || engine.external_cue.is_some()
                                || !engine.state.settings.studio.zones.is_empty()),
                    );
                    let began = Instant::now();
                    let fps = crate::device::effective_fps(engine.state.settings.fps);
                    crate::device::set_output_fps(fps);
                    let period = Duration::from_secs_f64(1.0 / fps as f64);
                    let late = began.saturating_duration_since(next);
                    if engine.state.powered
                        && (engine.state.capturing || engine.state.settings.mode == "rainbow")
                    {
                        engine.state.dropped_frames = engine
                            .state
                            .dropped_frames
                            .saturating_add(missed_slots(late, period));
                    }
                    engine.tick();
                    // Preserve cadence so repeated small delays cannot silently lower FPS.
                    let remainder =
                        Duration::from_nanos((late.as_nanos() % period.as_nanos()) as u64);
                    next = began + period.saturating_sub(remainder);
                    if last_event.elapsed() >= Duration::from_millis(100) {
                        emit(&engine.state);
                        last_event = Instant::now();
                    }
                }
            }
        })
        .expect("start lighting worker");
    Handle {
        tx,
        events,
        fleet: None,
    }
}
fn missed_slots(late: Duration, period: Duration) -> u64 {
    (late.as_nanos() / period.as_nanos().max(1)).min(u64::MAX as u128) as u64
}
#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "windows")]
    #[test]
    fn windows_worker_keeps_mta_before_and_between_capture_sessions() {
        use windows::Win32::{
            Foundation::RPC_E_CHANGED_MODE,
            System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_SINGLETHREADED},
        };
        let dir = tempfile::tempdir().unwrap();
        let (tx, rx) = mpsc::channel();
        let worker = start(
            Settings::default(),
            dir.path().join("settings.json"),
            String::new(),
            move |_| {
                // Model WASAPI's attempted STA initialization without opening an
                // audio device. It must find the worker's existing MTA each time.
                let attempt = unsafe { RoInitialize(RO_INIT_SINGLETHREADED) };
                if attempt.is_ok() {
                    unsafe { RoUninitialize() };
                }
                let mta = attempt.is_err_and(|e| e.code() == RPC_E_CHANGED_MODE);
                let capture_lease = crate::capture_windows::Apartment::new();
                let capture_ready = capture_lease.is_ok();
                drop(capture_lease);
                let _ = tx.send(mta && capture_ready);
            },
        );
        let mut observations = Vec::new();
        for _ in 0..3 {
            worker.request(Action::Snapshot).unwrap();
            observations.push(rx.recv_timeout(Duration::from_secs(5)).unwrap());
        }
        worker.request(Action::Shutdown).unwrap();
        assert!(observations.into_iter().all(|mta| mta));
    }

    #[test]
    fn controllers_keep_independent_settings_and_group_gain_without_overwriting_levels() {
        let dir = tempfile::tempdir().unwrap();
        let settings = Settings {
            color: "#ffffff".into(),
            brightness: 80,
            smoothing: 0,
            transition_limit: 0,
            ..Default::default()
        };
        let mut first = simulated(settings.clone(), dir.path().join("first/settings.json"));
        let mut second = simulated(
            Settings {
                brightness: 40,
                ..settings
            },
            dir.path().join("second/settings.json"),
        );
        first.action(Action::Start).unwrap();
        second.action(Action::Start).unwrap();
        first.action(Action::GroupBrightness { value: 50 }).unwrap();
        second
            .action(Action::GroupBrightness { value: 50 })
            .unwrap();
        first.tick();
        second.tick();
        assert_eq!(first.state.settings.brightness, 80);
        assert_eq!(second.state.settings.brightness, 40);
        assert!(first.state.colors[0][0] > second.state.colors[0][0]);
        first.controller.test_bus.as_mut().unwrap().online = false;
        first.tick();
        second.tick();
        assert!(second.state.status.connected);
        assert!(second.life.requested);
        let saved: u8 = serde_json::from_slice(
            &std::fs::read(dir.path().join("second/group-level.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(saved, 50);
    }
    #[test]
    fn handoff_closes_usb_pauses_rules_and_requires_explicit_start_after_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let settings = Settings {
            restore_on_start: true,
            ..Default::default()
        };
        let mut e = simulated(settings.clone(), path.clone());
        e.action(Action::Start).unwrap();
        let export = dir.path().join("handoff.json");
        e.action(Action::ExportHandoff {
            path: export.clone(),
        })
        .unwrap();
        assert!(!e.life.requested && !e.controller.status.connected && e.state.handoff_released);
        assert!(e.routines.config.paused && !e.routines.config.apps_enabled);
        assert!(e.action(Action::Connect).is_err());
        let text = std::fs::read_to_string(export).unwrap();
        let profile = crate::handoff::Profile::parse(&text).unwrap();
        assert!(
            !text.contains("deviceId")
                && !text.contains("screenId")
                && !text.contains("resumeCapture")
        );
        let mut next = simulated(settings, path);
        next.restore();
        assert!(!next.life.requested && next.state.handoff_released);
        next.action(Action::ImportHandoff { profile }).unwrap();
        assert!(!next.life.requested);
        next.action(Action::Start).unwrap();
        assert!(next.life.requested && !next.state.handoff_released);
    }
    #[test]
    fn application_rules_restore_scene_but_never_overwrite_manual_changes_or_off() {
        let dir = tempfile::tempdir().unwrap();
        let settings = Settings::default();
        let original = settings.color.clone();
        let mut e = simulated(settings.clone(), dir.path().join("settings.json"));
        e.action(Action::Start).unwrap();
        let mut target = settings;
        target.color = "#ff0000".into();
        let config = crate::routines::Config {
            apps_enabled: true,
            app_rules: vec![crate::routines::AppRule {
                app: "Fixture".into(),
                scene: crate::routines::Scene {
                    name: "Red".into(),
                    settings: SceneSettings::capture(&target),
                    source: SourceBinding::default(),
                },
            }],
            ..Default::default()
        };
        e.action(Action::RoutinesConfig {
            config: Some(config),
        })
        .unwrap();
        e.action(Action::Foreground {
            result: Ok("Fixture".into()),
        })
        .unwrap();
        assert_eq!(e.state.settings.color, "#ff0000");
        e.action(Action::PatchSettings {
            patch: SettingsPatch::default(),
            persist: true,
        })
        .unwrap();
        e.action(Action::Foreground {
            result: Ok("Other".into()),
        })
        .unwrap();
        assert_eq!(e.state.settings.color, original);
        e.action(Action::Foreground {
            result: Ok("Fixture".into()),
        })
        .unwrap();
        e.action(Action::Adjust {
            brightness: Some(17),
            color: Some("#00ff00".into()),
        })
        .unwrap();
        e.action(Action::Foreground {
            result: Ok("Fixture".into()),
        })
        .unwrap();
        e.action(Action::Foreground {
            result: Ok("Other".into()),
        })
        .unwrap();
        assert_eq!(e.state.settings.color, "#00ff00");
        assert_eq!(e.state.settings.brightness, 17);
        e.action(Action::Power { on: false }).unwrap();
        e.action(Action::Foreground {
            result: Ok("Fixture".into()),
        })
        .unwrap();
        assert!(!e.life.requested);
        assert_eq!(e.state.settings.color, "#00ff00");
    }
    #[test]
    fn solid_extensions_send_all_physical_pixels_and_restore_on_the_next_write() {
        use std::sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        };
        let dir = tempfile::tempdir().unwrap();
        let settings = Settings {
            count: 4,
            color: "#0000ff".into(),
            brightness: 100,
            max_brightness: 100,
            smoothing: 0,
            transition_limit: 0,
            offset: 1,
            reverse: true,
            ..Default::default()
        };
        let mut e = simulated(settings, dir.path().join("settings.json"));
        e.action(Action::Start).unwrap();
        e.tick();
        assert_eq!(
            &e.controller.test_bus.as_ref().unwrap().last_packet[..2],
            b"RB"
        );
        let guard = Arc::new(AtomicBool::new(true));
        let cue = crate::extensions::Overlay::cue(
            crate::extensions::Rule {
                event: "damage".into(),
                color: "#ff0000".into(),
                seconds: 2,
                start: 500,
                end: 1000,
            },
            guard.clone(),
        )
        .unwrap();
        e.action(Action::Extension {
            overlay: cue,
            cue: true,
        })
        .unwrap();
        e.tick();
        assert_eq!(
            e.state.colors,
            vec![[0, 0, 255], [0, 0, 255], [255, 0, 0], [255, 0, 0]]
        );
        let packet = &e.controller.test_bus.as_ref().unwrap().last_packet;
        assert_eq!(&packet[..2], b"SC");
        let expected = lighting::reorder(&e.state.colors, true, 1);
        for (i, color) in expected.iter().enumerate() {
            assert_eq!(&packet[7 + i * 5..10 + i * 5], color);
        }
        let writes = e.controller.test_bus.as_ref().unwrap().writes;
        guard.store(false, Ordering::Release);
        e.tick();
        let bus = e.controller.test_bus.as_ref().unwrap();
        assert_eq!(
            bus.writes,
            writes + 1,
            "do not wait for the solid heartbeat to restore"
        );
        assert_eq!(&bus.last_packet[..2], b"RB");
        assert_eq!(&bus.last_packet[6..9], &[0, 0, 255]);
        let guard = Arc::new(AtomicBool::new(true));
        e.action(Action::Extension {
            overlay: crate::extensions::Overlay::effect(
                crate::studio::Composition::default(),
                guard,
            )
            .unwrap(),
            cue: false,
        })
        .unwrap();
        e.tick();
        assert_eq!(
            &e.controller.test_bus.as_ref().unwrap().last_packet[..2],
            b"SC"
        );
    }
    #[test]
    fn remote_color_preserves_off_and_worker_checks_pause_and_revocation() {
        use std::sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        };
        let dir = tempfile::tempdir().unwrap();
        let mut e = simulated(Settings::default(), dir.path().join("settings.json"));
        let active = Arc::new(AtomicBool::new(true));
        let color = || Action::Remote {
            action: Box::new(Action::Adjust {
                brightness: None,
                color: Some("#123456".into()),
            }),
            active: active.clone(),
        };
        e.action(color()).unwrap();
        assert_eq!(e.state.settings.color, "#123456");
        assert!(!e.life.requested && !e.state.capturing);
        e.system_event(SystemEvent::Sleep(true));
        assert!(e.action(color()).is_err());
        e.action(Action::Remote {
            action: Box::new(Action::Power { on: false }),
            active: active.clone(),
        })
        .unwrap();
        e.system_event(SystemEvent::Sleep(false));
        active.store(false, Ordering::Release);
        assert!(e
            .action(Action::Remote {
                action: Box::new(Action::Start),
                active
            })
            .is_err());
        assert!(!e.life.requested);
    }
    #[test]
    fn extensions_restore_scene_on_revocation_stop_and_suspend_and_obey_comfort() {
        use std::sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        };
        let dir = tempfile::tempdir().unwrap();
        let settings = Settings {
            color: "#0000ff".into(),
            brightness: 100,
            max_brightness: 20,
            smoothing: 0,
            transition_limit: 0,
            ..Default::default()
        };
        let mut e = simulated(settings.clone(), dir.path().join("settings.json"));
        let guard = Arc::new(AtomicBool::new(true));
        let cue = || {
            crate::extensions::Overlay::cue(
                crate::extensions::Rule {
                    event: "damage".into(),
                    color: "#ff0000".into(),
                    seconds: 2,
                    start: 0,
                    end: 1000,
                },
                guard.clone(),
            )
            .unwrap()
        };
        assert!(e
            .action(Action::Extension {
                overlay: cue(),
                cue: true
            })
            .is_err());
        e.action(Action::Start).unwrap();
        e.action(Action::Automation {
            action: Box::new(Action::Extension {
                overlay: cue(),
                cue: true,
            }),
            active: guard.clone(),
            allow_capture: false,
        })
        .unwrap();
        e.tick();
        assert!(e
            .state
            .colors
            .iter()
            .all(|c| c[0] <= 51 && c[0] > 0 && c[2] == 0));
        assert_eq!(e.state.settings.color, settings.color);
        guard.store(false, Ordering::Release);
        e.tick();
        assert!(e.state.colors.iter().all(|c| c[0] == 0 && c[2] > 0));
        guard.store(true, Ordering::Release);
        e.action(Action::Extension {
            overlay: cue(),
            cue: true,
        })
        .unwrap();
        e.system_event(SystemEvent::Sleep(true));
        assert!(e.external_cue.is_none());
        e.system_event(SystemEvent::Sleep(false));
        e.action(Action::Start).unwrap();
        e.action(Action::Extension {
            overlay: cue(),
            cue: true,
        })
        .unwrap();
        e.action(Action::Power { on: false }).unwrap();
        e.tick();
        assert!(e.external_cue.is_none());
        assert!(!e.state.powered);
    }
    #[test]
    fn temporary_cues_preserve_scene_and_comfort_and_stop_cancels_fades() {
        let dir = tempfile::tempdir().unwrap();
        let settings = Settings {
            color: "#0000ff".into(),
            brightness: 100,
            max_brightness: 20,
            smoothing: 0,
            transition_limit: 0,
            ..Default::default()
        };
        let mut e = simulated(settings.clone(), dir.path().join("settings.json"));
        e.routines.config.manual_cue = crate::routines::Cue {
            enabled: true,
            color: "#ff0000".into(),
            seconds: 1,
        };
        e.action(Action::Start).unwrap();
        e.action(Action::Routine {
            command: "cue".into(),
        })
        .unwrap();
        e.tick();
        assert!(e.state.colors.iter().all(|c| c[0] <= 51 && c[2] == 0));
        assert!(e.state.colors[0][0] > 0);
        assert_eq!(e.state.settings, settings);
        e.fade = Some((Instant::now(), Duration::from_secs(10), false));
        e.action(Action::Power { on: false }).unwrap();
        e.tick();
        assert!(e.fade.is_none());
        assert!(!e.state.routines.cue_active);
        assert!(e.state.colors.iter().all(|c| *c == [0; 3]));
    }
    fn automated(action: Action, active: bool) -> Action {
        Action::Automation {
            action: Box::new(action),
            active: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(active)),
            allow_capture: false,
        }
    }
    #[test]
    fn automation_checks_current_capture_mode_and_revocation_before_side_effects() {
        let dir = tempfile::tempdir().unwrap();
        let mut e = simulated(Settings::default(), dir.path().join("settings.json"));
        for mode in ["screen", "music", "blend"] {
            e.action(Action::SelectMode { mode: mode.into() }).unwrap();
            let before = e.state.settings.clone();
            for action in [Action::Start, Action::Toggle] {
                assert!(e
                    .action(automated(action, true))
                    .err()
                    .unwrap()
                    .contains("Capture requires"));
                assert_eq!(e.state.settings, before);
                assert!(!e.life.requested && !e.state.capturing && !e.desired);
            }
        }
        let p = crate::scenes::starter_scenes()
            .profiles
            .into_iter()
            .find(|p| p.id == "cinema")
            .unwrap();
        e.action(Action::SelectMode {
            mode: "solid".into(),
        })
        .unwrap();
        assert!(e
            .action(automated(
                Action::ApplyProfile {
                    scene: p.settings,
                    source: p.source
                },
                true
            ))
            .is_err());
        assert_eq!(e.state.settings.mode, "solid");
        assert!(e
            .action(automated(Action::Start, false))
            .err()
            .unwrap()
            .contains("revoked"));
        assert_eq!(e.controller.test_bus.as_ref().unwrap().connects, 0);
        assert!(serde_json::from_str::<Action>(r#"{"type":"automation"}"#).is_err());
    }
    #[test]
    fn automation_preserves_comfort_limits_and_calibration_and_off() {
        let dir = tempfile::tempdir().unwrap();
        let mut e = simulated(
            Settings {
                mode: "solid".into(),
                max_brightness: 15,
                transition_limit: 70,
                ..Default::default()
            },
            dir.path().join("settings.json"),
        );
        e.action(automated(Action::Start, true)).unwrap();
        e.action(automated(
            Action::Adjust {
                brightness: Some(100),
                color: None,
            },
            true,
        ))
        .unwrap();
        let p = crate::scenes::starter_scenes()
            .profiles
            .into_iter()
            .find(|p| p.id == "warm-desk")
            .unwrap();
        e.action(automated(
            Action::ApplyProfile {
                scene: p.settings,
                source: p.source,
            },
            true,
        ))
        .unwrap();
        assert_eq!(
            (
                e.state.settings.max_brightness,
                e.state.settings.transition_limit
            ),
            (15, 70)
        );
        e.action(Action::CalibrationStart).unwrap();
        assert!(e
            .action(automated(
                Action::Adjust {
                    brightness: Some(50),
                    color: None
                },
                true
            ))
            .is_err());
        e.action(automated(Action::Power { on: false }, true))
            .unwrap();
        assert!(!e.state.calibrating && !e.state.requested && !e.state.capturing);
    }
    #[test]
    fn update_preparation_releases_device_preserves_saved_intent_and_allows_retry() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let mut e = simulated(Settings::default(), path);
        e.action(Action::StartMode {
            mode: "solid".into(),
        })
        .unwrap();
        assert!(e.state.powered && e.controller.status.connected);
        e.action(Action::PrepareUpdate).unwrap();
        assert!(!e.state.powered && !e.state.capturing && !e.controller.status.connected);
        assert!(!e.desired);
        assert!(SavedIntent::load(&e.intent_path()).unwrap().running);
        e.action(Action::StartMode {
            mode: "solid".into(),
        })
        .unwrap();
        assert!(e.state.powered && e.controller.status.connected);
        let original = e.state.settings.clone();
        e.action(Action::CalibrationStart).unwrap();
        e.action(Action::PrepareUpdate).unwrap();
        assert_eq!(Settings::load(&e.path).unwrap(), original);
        assert!(!e.state.calibrating);
        assert!(serde_json::from_str::<Action>(r#"{"type":"prepareUpdate"}"#).is_err());
        assert!(serde_json::from_str::<Action>(r#"{"type":"shutdown"}"#).is_err());
    }
    #[test]
    fn modes_survive_restart_without_restoring_hardware_or_capture_consent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let mut e = Engine::new(Settings::default(), path.clone(), String::new());
        e.action(Action::SelectMode {
            mode: "music".into(),
        })
        .unwrap();
        let mut music = e.state.settings.clone();
        music.brightness = 17;
        music.input_gain = 8;
        music.palette = "ocean".into();
        e.action(Action::Settings { settings: music }).unwrap();
        e.action(Action::SelectMode {
            mode: "rainbow".into(),
        })
        .unwrap();
        let mut effects = e.state.settings.clone();
        effects.brightness = 60;
        effects.effect_reverse = true;
        effects.count = 90;
        effects.max_brightness = 30;
        e.action(Action::Settings { settings: effects }).unwrap();
        let mut restarted = Engine::new(Settings::load(&path).unwrap(), path, String::new());
        restarted
            .action(Action::SelectMode {
                mode: "music".into(),
            })
            .unwrap();
        let s = &restarted.state.settings;
        assert_eq!(
            (s.brightness, s.input_gain, s.palette.as_str()),
            (17, 8, "ocean")
        );
        assert_eq!((s.count, s.max_brightness), (90, 30));
        assert!(!restarted.state.capturing && !restarted.state.requested);
        restarted
            .action(Action::SelectMode {
                mode: "rainbow".into(),
            })
            .unwrap();
        assert_eq!(restarted.state.settings.brightness, 60);
        assert!(restarted.state.settings.effect_reverse);
        let before = restarted.state.settings.clone();
        assert!(restarted
            .action(Action::SelectMode {
                mode: "invalid".into()
            })
            .is_err());
        assert_eq!(restarted.state.settings, before);
    }
    #[test]
    fn frame_deadlines_count_only_whole_missed_slots() {
        let period = Duration::from_millis(50);
        assert_eq!(missed_slots(Duration::from_millis(49), period), 0);
        assert_eq!(missed_slots(Duration::from_millis(50), period), 1);
        assert_eq!(missed_slots(Duration::from_millis(151), period), 3);
    }
    fn simulated(settings: Settings, path: PathBuf) -> Engine {
        let mut e = Engine::new(settings, path, String::new());
        e.controller.test_bus = Some(crate::device::TestBus {
            online: true,
            connects: 0,
            writes: 0,
            last_packet: vec![],
        });
        e
    }
    #[test]
    fn studio_zones_use_frames_respect_off_and_capture_and_preserve_scene_data() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Settings {
            count: 4,
            brightness: 100,
            max_brightness: 20,
            ..Default::default()
        };
        s.studio.zones.push(crate::studio::Zone {
            name: "Left".into(),
            start: 0,
            end: 500,
            color: "#ff0000".into(),
            effect: None,
        });
        s.studio.zones.push(crate::studio::Zone {
            name: "Right".into(),
            start: 500,
            end: 1000,
            color: "#0000ff".into(),
            effect: None,
        });
        let mut e = simulated(s.clone(), dir.path().join("settings.json"));
        e.tick();
        assert!(e.state.colors.iter().all(|c| *c == [0; 3]));
        e.action(Action::Start).unwrap();
        e.tick();
        assert_eq!(
            e.state.colors,
            [[51, 0, 0], [51, 0, 0], [0, 0, 51], [0, 0, 51]]
        );
        e.action(Action::Power { on: false }).unwrap();
        e.tick();
        assert!(e.state.colors.iter().all(|c| *c == [0; 3]));
        s.mode = "screen".into();
        e.action(Action::Settings {
            settings: s.clone(),
        })
        .unwrap();
        e.life.requested = true;
        e.tick();
        assert!(!e.state.capturing);
        assert!(
            e.state.colors.iter().all(|c| *c == [0; 3]),
            "Zones cannot bypass capture consent"
        );
        let scene = SceneSettings::capture(&s);
        let serialized = serde_json::to_string(&scene).unwrap();
        let restored: SceneSettings = serde_json::from_str(&serialized).unwrap();
        assert_eq!(restored.studio, s.studio);
        assert_eq!(
            restored
                .apply(&Settings {
                    max_brightness: 5,
                    ..Default::default()
                })
                .max_brightness,
            5
        );
    }
    #[test]
    fn finite_effects_restart_and_color_edits_exit_bias() {
        let dir = tempfile::tempdir().unwrap();
        let mut e = simulated(Settings::default(), dir.path().join("settings.json"));
        e.started = Instant::now() - Duration::from_secs(300);
        let mut s = e.state.settings.clone();
        s.effect = "custom".into();
        s.mode = "rainbow".into();
        s.studio.effect.repeats = 1;
        e.action(Action::Settings { settings: s }).unwrap();
        assert!(e.started.elapsed() < Duration::from_secs(1));
        let old = e.started;
        e.action(Action::Adjust {
            brightness: Some(20),
            color: None,
        })
        .unwrap();
        assert_eq!(e.started, old, "Brightness must not restart an effect");
        let mut s = e.state.settings.clone();
        s.mode = "solid".into();
        s.studio.bias.enabled = true;
        e.action(Action::Settings { settings: s }).unwrap();
        e.action(Action::Adjust {
            brightness: None,
            color: Some("#ff0000".into()),
        })
        .unwrap();
        assert!(!e.state.settings.studio.bias.enabled);
    }
    #[test]
    fn changing_capture_region_revokes_prior_capture_consent() {
        let dir = tempfile::tempdir().unwrap();
        let mut e = simulated(
            Settings {
                mode: "screen".into(),
                ..Default::default()
            },
            dir.path().join("settings.json"),
        );
        e.life.requested = true;
        e.life.capture_allowed = true;
        e.state.capturing = true;
        let source = crate::capture_source::Source::Region {
            left: 250,
            top: 0,
            width: 500,
            height: 1000,
        };
        e.action(Action::PatchSettings {
            patch: SettingsPatch(serde_json::Map::from_iter([(
                "captureSource".into(),
                serde_json::to_value(source).unwrap(),
            )])),
            persist: true,
        })
        .unwrap();
        assert!(!e.state.capturing && !e.life.capture_allowed);
        assert!(e.state.capture_requires_start);
        let portable = serde_json::to_value(SceneSettings::capture(&e.state.settings)).unwrap();
        assert!(
            portable.get("captureSource").is_none(),
            "Local source identities are not portable scene data"
        );
    }
    #[test]
    fn changing_only_window_crop_requires_fresh_capture_consent() {
        let dir = tempfile::tempdir().unwrap();
        let mut source = crate::capture_source::Source::Window {
            id: 1,
            pid: 2,
            app: "local-browser".into(),
            session: "local-session".into(),
            crop: None,
        };
        let mut e = simulated(
            Settings {
                mode: "screen".into(),
                capture_source: source.clone(),
                ..Default::default()
            },
            dir.path().join("settings.json"),
        );
        e.life.requested = true;
        e.life.capture_allowed = true;
        e.state.capturing = true;
        let crate::capture_source::Source::Window { crop, .. } = &mut source else {
            unreachable!()
        };
        *crop = Some(crate::capture_source::Rectangle {
            left: 200,
            top: 200,
            width: 600,
            height: 600,
        });
        e.action(Action::PatchSettings {
            patch: SettingsPatch(serde_json::Map::from_iter([(
                "captureSource".into(),
                serde_json::to_value(&source).unwrap(),
            )])),
            persist: true,
        })
        .unwrap();
        assert_eq!(e.state.settings.capture_source, source);
        assert!(!e.state.capturing && !e.life.capture_allowed);
        assert!(e.state.capture_requires_start);
    }
    #[test]
    fn offline_replay_stops_capture_and_respects_stop_and_comfort() {
        let dir = tempfile::tempdir().unwrap();
        let mut e = simulated(
            Settings {
                mode: "screen".into(),
                count: 2,
                brightness: 100,
                max_brightness: 10,
                ..Default::default()
            },
            dir.path().join("settings.json"),
        );
        let sequence:crate::sequence::Sequence=serde_json::from_value(serde_json::json!({"format":"dynamiclight-rgb","version":1,"id":"test","count":2,"sides":3,"durationMs":1000,"frames":[{"ms":0,"colors":[[255,0,0],[0,0,255]]}]})).unwrap();
        e.action(Action::LoadSequence { sequence }).unwrap();
        assert!(!e.state.requested && !e.state.status.connected);
        e.state.capturing = true;
        e.life.capture_allowed = true;
        let mut s = e.state.settings.clone();
        s.mode = "replay".into();
        s.replay.clip = "test".into();
        s.smoothing = 0;
        e.action(Action::Settings { settings: s }).unwrap();
        assert!(!e.state.capturing && !e.life.capture_allowed);
        e.action(Action::Start).unwrap();
        e.tick();
        assert_eq!(e.state.colors, [[26, 0, 0], [0, 0, 26]]);
        assert!(!e.state.capturing);
        assert!(e.action(Action::RecordStart { seconds: 1 }).is_err());
        e.action(Action::Power { on: false }).unwrap();
        e.tick();
        assert!(e.state.colors.iter().all(|c| *c == [0; 3]));
    }
    #[test]
    fn start_mode_restores_memory_and_respects_calibration_and_stop() {
        let dir = tempfile::tempdir().unwrap();
        let mut e = simulated(
            Settings {
                count: 37,
                brightness: 23,
                max_brightness: 40,
                ..Default::default()
            },
            dir.path().join("settings.json"),
        );
        e.action(Action::SelectMode {
            mode: "rainbow".into(),
        })
        .unwrap();
        e.action(Action::Adjust {
            brightness: Some(61),
            color: None,
        })
        .unwrap();
        e.action(Action::StartMode {
            mode: "solid".into(),
        })
        .unwrap();
        assert!(e.state.requested && e.state.powered && e.state.status.connected);
        assert!(!e.state.capturing);
        assert_eq!(e.state.settings.brightness, 23);
        assert_eq!(
            (e.state.settings.count, e.state.settings.max_brightness),
            (37, 40)
        );
        e.action(Action::StartMode {
            mode: "rainbow".into(),
        })
        .unwrap();
        assert_eq!(e.state.settings.brightness, 61);
        let before = e.state.settings.clone();
        assert!(e
            .action(Action::StartMode {
                mode: "invalid".into()
            })
            .is_err());
        assert_eq!(e.state.settings, before);
        e.action(Action::CalibrationStart).unwrap();
        assert!(e
            .action(Action::StartMode {
                mode: "solid".into()
            })
            .is_err());
        assert!(e.state.calibrating);
        e.action(Action::Power { on: false }).unwrap();
        e.tick();
        assert!(!e.state.requested && !e.state.powered && !e.state.calibrating);
    }
    #[test]
    fn blend_display_changes_keep_audio_and_stop_prevents_source_recovery() {
        let dir = tempfile::tempdir().unwrap();
        let mut e = simulated(
            Settings {
                mode: "blend".into(),
                resume_capture: false,
                ..Default::default()
            },
            dir.path().join("settings.json"),
        );
        e.life.requested = true;
        e.life.capture_allowed = true;
        e.state.capturing = true;
        e.audio = Some(Audio::test_tone(0.02));
        e.system_event(SystemEvent::SourcesChanged);
        assert!(e.audio.is_some() && e.state.capturing && e.state.requested);
        assert!(e.state.error.contains("using music"));
        e.lose_blend_source(false);
        e.update_lifecycle();
        assert!(!e.state.capturing && e.state.capture_requires_start);
        e.action(Action::Power { on: false }).unwrap();
        e.capture_retry = Instant::now() - Duration::from_secs(10);
        e.tick();
        assert!(!e.state.requested && !e.state.capturing);
        assert!(e.audio.is_none() && e.screen.is_none());
    }
    #[test]
    fn interleaved_field_edits_and_delayed_commits_preserve_latest_state() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let mut e = simulated(Settings::default(), path.clone());
        let patch = |value, persist| Action::PatchSettings {
            patch: serde_json::from_value(value).unwrap(),
            persist,
        };
        // Studio previews a color while Quick controls changes brightness.
        e.action(patch(serde_json::json!({"color":"#123456"}), false))
            .unwrap();
        e.action(Action::PreviewBrightness { brightness: 72 })
            .unwrap();
        e.action(patch(serde_json::json!({}), true)).unwrap();
        assert_eq!(Settings::load(&path).unwrap().brightness, 72);
        assert_eq!(Settings::load(&path).unwrap().color, "#123456");
        // Same-field edits have explicit command-order precedence; an idle save
        // must never reapply the old preview after another window's change.
        e.action(patch(serde_json::json!({"brightness":23}), false))
            .unwrap();
        e.action(Action::Adjust {
            brightness: Some(41),
            color: None,
        })
        .unwrap();
        e.action(patch(serde_json::json!({}), true)).unwrap();
        assert_eq!(Settings::load(&path).unwrap().brightness, 41);
        let before = e.state.settings.clone();
        for invalid in [
            serde_json::json!({"brightness":101}),
            serde_json::json!({"modeSettings":{}}),
            serde_json::json!({"typo":1}),
            serde_json::json!({"brightness":null}),
        ] {
            assert!(e.action(patch(invalid, true)).is_err());
            assert_eq!(e.state.settings, before);
            assert_eq!(Settings::load(&path).unwrap(), before);
        }
        e.action(Action::Power { on: false }).unwrap();
        e.action(patch(serde_json::json!({"brightness":50}), true))
            .unwrap();
        assert!(!e.state.requested && !e.state.capturing);
        e.action(Action::CalibrationStart).unwrap();
        assert!(e
            .action(patch(serde_json::json!({"count":100}), false))
            .is_err());
        e.action(Action::Power { on: false }).unwrap();
        assert!(!e.state.calibrating && !e.state.requested);
    }
    #[test]
    fn failed_patch_persistence_leaves_runtime_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let mut e = simulated(Settings::default(), dir.path().to_path_buf());
        let before = e.state.settings.clone();
        assert!(e
            .action(Action::PatchSettings {
                patch: serde_json::from_value(serde_json::json!({"brightness":70})).unwrap(),
                persist: true,
            })
            .is_err());
        assert_eq!(e.state.settings, before);
        assert!(!e.state.requested);
    }
    #[test]
    fn tray_and_profile_actions_preserve_current_layout_and_stop_intent() {
        let dir = tempfile::tempdir().unwrap();
        let current = Settings {
            count: 37,
            max_brightness: 15,
            ..Default::default()
        };
        let mut e = simulated(current.clone(), dir.path().join("settings.json"));
        e.action(Action::ApplyProfile {
            scene: SceneSettings::capture(&Settings {
                brightness: 90,
                count: 100,
                max_brightness: 100,
                restore_on_start: true,
                ..Default::default()
            }),
            source: SourceBinding::default(),
        })
        .unwrap();
        assert!(e.state.requested);
        assert_eq!(e.state.settings.count, 37);
        assert_eq!(e.state.settings.max_brightness, 15);
        assert!(!e.state.settings.restore_on_start);
        e.action(Action::Adjust {
            brightness: Some(25),
            color: None,
        })
        .unwrap();
        assert_eq!(e.state.settings.brightness, 25);
        assert_eq!(e.state.settings.count, 37);
        e.action(Action::Toggle).unwrap();
        assert!(!e.state.requested);
        e.action(Action::Adjust {
            brightness: Some(50),
            color: None,
        })
        .unwrap();
        assert!(!e.state.requested);
        e.action(Action::CalibrationStart).unwrap();
        assert!(e
            .action(Action::ApplyProfile {
                scene: SceneSettings::capture(&current),
                source: SourceBinding::default()
            })
            .is_err());
        e.action(Action::Toggle).unwrap();
        assert!(!e.state.calibrating && !e.state.requested);
    }
    #[test]
    fn calibration_is_temporary_and_stop_cancels_restoration() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let original = Settings {
            mode: "rainbow".into(),
            minimum_glow: 10,
            ..Default::default()
        };
        original.save(&path).unwrap();
        let mut e = simulated(original.clone(), path.clone());
        e.action(Action::Start).unwrap();
        e.action(Action::CalibrationStart).unwrap();
        let draft = Settings {
            count: 10,
            reverse: true,
            offset: 3,
            ..original.clone()
        };
        e.action(Action::CalibrationStep {
            settings: draft.clone(),
            led: Some(4),
        })
        .unwrap();
        e.tick();
        assert_eq!(e.state.colors.iter().filter(|c| **c != [0; 3]).count(), 1);
        assert_ne!(e.state.colors[4], [0; 3]);
        assert_eq!(Settings::load(&path).unwrap(), original);
        e.action(Action::CalibrationEnd { apply: false }).unwrap();
        assert_eq!(e.state.settings, original);
        assert!(e.state.requested);
        e.action(Action::CalibrationStart).unwrap();
        e.action(Action::CalibrationStep {
            settings: draft,
            led: None,
        })
        .unwrap();
        e.action(Action::CalibrationEnd { apply: true }).unwrap();
        let saved = Settings::load(&path).unwrap();
        assert_eq!(saved.count, 10);
        assert_eq!(saved.mode, "rainbow");
        assert_eq!(saved.brightness, original.brightness);
        e.action(Action::CalibrationStart).unwrap();
        e.action(Action::Power { on: false }).unwrap();
        e.tick();
        assert!(!e.state.calibrating && !e.state.requested);
        assert!(e.state.colors.iter().all(|c| *c == [0; 3]));
        e.action(Action::CalibrationEnd { apply: false }).unwrap();
        assert!(!e.state.requested);
    }
    #[test]
    fn calibration_pause_restores_scene_without_bypassing_pause() {
        let dir = tempfile::tempdir().unwrap();
        let mut e = simulated(Settings::default(), dir.path().join("settings.json"));
        e.action(Action::Start).unwrap();
        e.action(Action::CalibrationStart).unwrap();
        e.system_event(SystemEvent::Locked(true));
        assert!(!e.state.calibrating && !e.state.powered && e.state.requested);
        assert_eq!(e.state.settings.brightness, Settings::default().brightness);
    }
    #[test]
    fn usb_failure_retries_only_after_delay_and_stop_cancels_recovery() {
        let dir = tempfile::tempdir().unwrap();
        let mut e = simulated(
            Settings {
                mode: "rainbow".into(),
                ..Default::default()
            },
            dir.path().join("settings.json"),
        );
        e.action(Action::Start).unwrap();
        e.tick();
        assert_eq!(e.controller.test_bus.as_ref().unwrap().connects, 1);
        e.controller.test_bus.as_mut().unwrap().online = false;
        e.tick();
        assert!(e.state.requested && e.state.recovering);
        assert_eq!(e.state.write_errors, 1);
        let sent = e.state.frames_written;
        e.controller.test_bus.as_mut().unwrap().online = true;
        e.tick();
        assert_eq!(
            e.controller.test_bus.as_ref().unwrap().connects,
            1,
            "no busy retry loop"
        );
        e.retry = Instant::now() - Duration::from_secs(3);
        e.tick();
        assert!(e.state.status.connected);
        assert!(e.state.frames_written > sent);
        assert!(!e.state.recovering);
        assert!(e.state.error.is_empty());
        e.controller.test_bus.as_mut().unwrap().online = false;
        e.tick();
        e.action(Action::Power { on: false }).unwrap();
        e.controller.test_bus.as_mut().unwrap().online = true;
        e.retry = Instant::now() - Duration::from_secs(3);
        e.tick();
        assert_eq!(e.controller.test_bus.as_ref().unwrap().connects, 2);
        assert!(!e.state.requested && !e.state.status.connected);
    }
    #[test]
    fn startup_waits_for_initial_desktop_state_and_respects_lock() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        SavedIntent { running: true }
            .save(&path.with_file_name("run-state.json"))
            .unwrap();
        let mut e = simulated(
            Settings {
                restore_on_start: true,
                ..Default::default()
            },
            path,
        );
        e.restore();
        e.retry = Instant::now() - Duration::from_secs(10);
        e.tick();
        assert_eq!(e.controller.test_bus.as_ref().unwrap().connects, 0);
        e.system_event(SystemEvent::Locked(true));
        e.system_event(SystemEvent::Ready);
        e.tick();
        assert_eq!(e.controller.test_bus.as_ref().unwrap().connects, 0);
        e.system_event(SystemEvent::Locked(false));
        e.tick();
        assert!(e.state.status.connected && e.state.powered);
    }
    #[test]
    fn suspend_closes_usb_before_teardown_and_respects_blackout_preference() {
        let dir = tempfile::tempdir().unwrap();
        for blackout in [false, true] {
            let mut e = simulated(
                Settings {
                    off_on_sleep: blackout,
                    ..Default::default()
                },
                dir.path().join("settings.json"),
            );
            e.action(Action::Start).unwrap();
            e.tick();
            let writes = e.controller.test_bus.as_ref().unwrap().writes;
            e.system_event(SystemEvent::Sleep(true));
            e.tick();
            assert!(!e.state.status.connected);
            assert_eq!(
                e.controller.test_bus.as_ref().unwrap().writes,
                writes + usize::from(blackout)
            );
            e.system_event(SystemEvent::Sleep(false));
            e.tick();
            assert!(e.state.status.connected && e.state.powered);
        }
    }
    #[test]
    fn capture_is_never_implicit_and_sleep_requires_restart() {
        let dir = tempfile::tempdir().unwrap();
        let mut e = Engine::new(
            Settings {
                mode: "screen".into(),
                ..Settings::default()
            },
            dir.path().join("settings.json"),
            String::new(),
        );
        e.tick();
        assert!(!e.state.capturing);
        assert!(e.state.colors.iter().all(|c| *c == [0; 3]));
        e.system_event(SystemEvent::Sleep(true));
        e.system_event(SystemEvent::Sleep(false));
        assert!(!e.state.powered);
        assert!(!e.state.capturing);
        e.action(Action::Power { on: true }).unwrap();
        assert!(!e.state.capturing);
    }
    #[test]
    fn invalid_settings_leave_state_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let initial = Settings::default();
        let mut e = Engine::new(
            initial.clone(),
            dir.path().join("settings.json"),
            String::new(),
        );
        let invalid = Settings {
            count: 0,
            ..initial.clone()
        };
        assert!(e.action(Action::Settings { settings: invalid }).is_err());
        assert_eq!(e.state.settings, initial);
    }
    #[test]
    fn live_settings_apply_without_disk_writes_then_commit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let mut e = Engine::new(Settings::default(), path.clone(), String::new());
        let s = Settings {
            mode: "rainbow".into(),
            effect: "aurora".into(),
            brightness: 80,
            ..Settings::default()
        };
        e.action(Action::PreviewSettings {
            settings: s.clone(),
        })
        .unwrap();
        e.tick();
        assert!(!path.exists());
        assert!(
            e.state.colors.iter().all(|c| *c == [0; 3]),
            "editing settings must not start lighting"
        );
        e.action(Action::Power { on: true }).unwrap();
        e.tick();
        assert!(e.state.colors.iter().any(|c| *c != [0; 3]));
        assert!(!e.state.status.connected);
        assert_eq!(e.state.frames_written, 0);
        assert_eq!(e.state.last_write_ms, None);
        e.action(Action::Settings {
            settings: s.clone(),
        })
        .unwrap();
        let saved = Settings::load(&path).unwrap();
        assert_eq!(saved, e.state.settings);
        assert_eq!(saved.brightness, s.brightness);
        e.action(Action::Power { on: false }).unwrap();
        e.tick();
        assert!(e.state.colors.iter().all(|c| *c == [0; 3]));
    }
    #[test]
    fn startup_requires_both_opt_in_and_saved_running_intent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        SavedIntent { running: true }
            .save(&path.with_file_name("run-state.json"))
            .unwrap();
        for (restore, capture) in [(false, false), (true, false), (true, true)] {
            let mut engine = Engine::new(
                Settings {
                    mode: "screen".into(),
                    restore_on_start: restore,
                    resume_capture: capture,
                    ..Default::default()
                },
                path.clone(),
                String::new(),
            );
            engine.restore();
            assert_eq!(engine.state.requested, restore);
            assert_eq!(engine.state.capture_requires_start, restore && !capture);
            assert!(
                !engine.state.capturing,
                "restore must wait for native session observers"
            );
        }
        SavedIntent { running: false }
            .save(&path.with_file_name("run-state.json"))
            .unwrap();
        let mut engine = Engine::new(
            Settings {
                restore_on_start: true,
                resume_capture: true,
                ..Default::default()
            },
            path,
            String::new(),
        );
        engine.restore();
        assert!(!engine.state.requested);
    }
    #[test]
    fn nested_pause_resume_and_stop_preserve_user_intent() {
        let dir = tempfile::tempdir().unwrap();
        let mut e = Engine::new(
            Settings::default(),
            dir.path().join("settings.json"),
            String::new(),
        );
        e.action(Action::Power { on: true }).unwrap();
        e.system_event(SystemEvent::Locked(true));
        e.system_event(SystemEvent::DisplayOff(true));
        e.system_event(SystemEvent::Sleep(true));
        e.system_event(SystemEvent::Sleep(true)); // Duplicate suspend cannot overwrite intent.
        e.system_event(SystemEvent::Sleep(false));
        assert!(!e.state.powered);
        assert!(e.state.requested);
        e.system_event(SystemEvent::Locked(false));
        assert!(!e.state.powered);
        e.system_event(SystemEvent::DisplayOff(false));
        assert!(e.state.powered);
        e.system_event(SystemEvent::Sleep(true));
        e.action(Action::Power { on: false }).unwrap();
        e.system_event(SystemEvent::Sleep(false));
        assert!(!e.state.requested);
        assert!(!e.state.powered);
        assert!(!SavedIntent::load(&e.intent_path()).unwrap().running);
    }
    #[test]
    fn recovery_consent_cannot_override_stop_or_changed_source() {
        let dir = tempfile::tempdir().unwrap();
        let mut e = Engine::new(
            Settings {
                mode: "music".into(),
                resume_capture: true,
                ..Default::default()
            },
            dir.path().join("settings.json"),
            String::new(),
        );
        e.life.requested = true;
        e.life.capture_allowed = true;
        e.system_event(SystemEvent::Sleep(true));
        e.system_event(SystemEvent::Sleep(false));
        assert!(e.life.capture_allowed);
        e.action(Action::Settings {
            settings: Settings {
                mode: "screen".into(),
                resume_capture: true,
                ..Default::default()
            },
        })
        .unwrap();
        assert!(
            !e.life.capture_allowed,
            "changing source needs an explicit start"
        );
        e.action(Action::Power { on: false }).unwrap();
        e.system_event(SystemEvent::SourcesChanged);
        assert!(!e.state.recovering);
        assert!(!e.state.requested);
    }
}
