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
    pub settings: Settings,
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
    tx: SyncSender<Message>,
    events: mpsc::Sender<(SystemEvent, SyncSender<()>)>,
}
impl Handle {
    pub fn request(&self, action: Action) -> Result<Snapshot, String> {
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
}
impl Engine {
    pub fn new(settings: Settings, path: PathBuf, warning: String) -> Self {
        let controller = Controller::new();
        Self {
            state: Snapshot {
                settings,
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
                self.controller.connect()?;
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
                let screen = ScreenCapture::start(&self.state.settings.screen_id);
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
            "screen" => self.screen = Some(ScreenCapture::start(&self.state.settings.screen_id)?),
            "music" => {
                self.audio = Some(Audio::start(self.state.settings.audio_source == "system")?)
            }
            _ => return Err("Select Screen or Music before starting capture".into()),
        }
        self.state.capturing = true;
        if self.state.settings.mode == "blend" {
            self.blend_sources = (
                crate::capture::source_signature(&self.state.settings.screen_id).ok(),
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
                crate::capture::source_signature(&self.state.settings.screen_id),
                crate::audio::source_signature(self.state.settings.audio_source == "system")
            )),
            "screen" => crate::capture::source_signature(&self.state.settings.screen_id),
            "music" => crate::audio::source_signature(self.state.settings.audio_source == "system"),
            _ => Ok(String::new()),
        }
    }
    fn start_requested(&mut self) -> Result<(), String> {
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
            self.controller.connect()?;
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
        if persist {
            settings.save(&self.path)?;
        }
        if settings.mode != self.state.settings.mode
            || settings.audio_source != self.state.settings.audio_source
            || settings.screen_id != self.state.settings.screen_id
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
        self.state.settings = settings;
        if !was_paused && !self.life.pause_reason(&self.state.settings).is_empty() {
            self.interrupt_capture();
        }
        self.last_solid = None;
        self.state.error.clear();
        Ok(())
    }
    pub fn action(&mut self, action: Action) -> Result<Snapshot, String> {
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
                self.begin_calibration()?;
            }
            Action::CalibrationStep { settings, led } => {
                self.step_calibration(settings, led)?;
            }
            Action::CalibrationEnd { apply } => self.end_calibration(apply, true)?,
            Action::Start => {
                if let Err(e) = self.start_requested() {
                    self.state.error = e;
                }
            }
            Action::Connect => {
                self.state.error.clear();
                self.desired = true;
                self.retry = Instant::now();
                self.controller.connect()?;
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
        Ok(self.state.clone())
    }
    fn reconcile_capture(&mut self) {
        if self.state.capturing && self.source_check.elapsed() >= Duration::from_secs(2) {
            self.source_check = Instant::now();
            if self.state.settings.mode == "blend" {
                let sources = (
                    crate::capture::source_signature(&self.state.settings.screen_id).ok(),
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
                    self.screen = ScreenCapture::start(&self.state.settings.screen_id).ok();
                }
                if self.audio.is_none() {
                    self.audio = Audio::start(self.state.settings.audio_source == "system").ok();
                }
                self.blend_sources = (
                    crate::capture::source_signature(&self.state.settings.screen_id).ok(),
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
                        let (x, y) = lighting::positions(s.count, s.sides, 16.0 / 9.0)[i];
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
            "solid" => Ok(vec![lighting::hex(&s.color); s.count]),
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
        if self.desired
            && self.life.pause_reason(&self.state.settings).is_empty()
            && !self.controller.status.connected
            && self.retry.elapsed() >= Duration::from_secs(3)
        {
            self.retry = Instant::now();
            match self.controller.connect() {
                Ok(()) => self.state.error.clear(),
                Err(e) => self.state.error = format!("Waiting for strip: {e}"),
            }
            self.last_solid = None;
        }
        self.reconcile_capture();
        let generated = self.generate_frame(dt);
        match generated {
            Ok(colors) => {
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
                    self.previous =
                        lighting::transform(&colors, &self.previous, &self.state.settings, dt);
                }
                self.state.colors = self
                    .previous
                    .iter()
                    .map(|rgb| rgb.map(|c| c.round().clamp(0.0, 255.0) as u8))
                    .collect();
            }
            Err(e) => {
                self.interrupt_capture();
                self.state.error = format!("Capture unavailable: {e}");
            }
        }
        if self.controller.status.connected && !self.life.sleeping {
            let write_started = Instant::now();
            let s = &self.state.settings;
            let active = self.state.powered
                && (self.state.calibrating || s.mode == "rainbow" || self.state.capturing);
            let result = if active {
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
                        let _ = msg.reply.send(result);
                        emit(&engine.state);
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
                            && (engine.state.capturing || engine.state.settings.mode == "rainbow"),
                    );
                    let began = Instant::now();
                    let period = Duration::from_secs_f64(1.0 / engine.state.settings.fps as f64);
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
    Handle { tx, events }
}
fn missed_slots(late: Duration, period: Duration) -> u64 {
    (late.as_nanos() / period.as_nanos().max(1)).min(u64::MAX as u128) as u64
}
#[cfg(test)]
mod tests {
    use super::*;
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
        });
        e
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
