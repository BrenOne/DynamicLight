use crate::{
    audio::Audio,
    capture::ScreenCapture,
    device::{Controller, Status},
    lighting,
    protocol::Rgb,
    settings::Settings,
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
    Connect,
    Disconnect,
    Settings {
        settings: Settings,
    },
    Power {
        on: bool,
    },
    StartCapture,
    StopCapture,
    #[serde(skip)]
    Suspend,
    #[serde(skip)]
    Resume,
    #[serde(skip)]
    Shutdown,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub settings: Settings,
    pub status: Status,
    pub powered: bool,
    pub capturing: bool,
    pub colors: Vec<Rgb>,
    pub fps: u32,
    pub error: String,
    pub capture_size: (u32, u32),
    pub audio_samples: u64,
    pub frames_written: u64,
}
pub struct Message {
    action: Action,
    reply: SyncSender<Result<Snapshot, String>>,
}
#[derive(Clone)]
pub struct Handle {
    tx: SyncSender<Message>,
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
    pub fn power_event(&self, sleeping: bool) {
        let (tx, rx) = mpsc::sync_channel(1);
        if self
            .tx
            .try_send(Message {
                action: if sleeping {
                    Action::Suspend
                } else {
                    Action::Resume
                },
                reply: tx,
            })
            .is_ok()
        {
            let _ = rx.recv_timeout(Duration::from_secs(2));
        }
    }
}
pub struct Engine {
    pub state: Snapshot,
    controller: Controller,
    path: PathBuf,
    desired: bool,
    suspended: bool,
    screen: Option<ScreenCapture>,
    audio: Option<Audio>,
    previous: Vec<[f32; 3]>,
    started: Instant,
    retry: Instant,
    last_solid: Option<(Rgb, usize)>,
    heartbeat: Instant,
    fps_start: Instant,
    fps_frames: u32,
}
impl Engine {
    pub fn new(settings: Settings, path: PathBuf, warning: String) -> Self {
        let controller = Controller::new();
        Self {
            state: Snapshot {
                settings,
                status: controller.status.clone(),
                powered: true,
                capturing: false,
                colors: vec![],
                fps: 0,
                error: warning,
                capture_size: (0, 0),
                audio_samples: 0,
                frames_written: 0,
            },
            controller,
            path,
            desired: false,
            suspended: false,
            screen: None,
            audio: None,
            previous: vec![],
            started: Instant::now(),
            retry: Instant::now(),
            last_solid: None,
            heartbeat: Instant::now(),
            fps_start: Instant::now(),
            fps_frames: 0,
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
    }
    pub fn action(&mut self, action: Action) -> Result<Snapshot, String> {
        match action {
            Action::Snapshot => {}
            Action::Connect => {
                self.state.error.clear();
                self.desired = true;
                self.retry = Instant::now();
                self.controller.connect()?;
                self.last_solid = None;
            }
            Action::Disconnect => {
                self.desired = false;
                self.stop();
                self.controller.close("Disconnected");
            }
            Action::Settings { settings } => {
                settings.validate()?;
                settings.save(&self.path)?;
                if settings.mode != self.state.settings.mode
                    || settings.audio_source != self.state.settings.audio_source
                    || settings.screen_id != self.state.settings.screen_id
                {
                    self.stop();
                }
                if settings.count != self.state.settings.count {
                    self.black();
                }
                self.state.settings = settings;
                self.previous.clear();
                self.last_solid = None;
                self.state.error.clear();
            }
            Action::Power { on } => {
                if self.suspended && on {
                    return Err("Computer is sleeping".into());
                }
                self.state.powered = on;
                self.last_solid = None;
                self.state.error.clear();
                if !on {
                    self.stop();
                }
            }
            Action::StartCapture => {
                if !self.state.powered || self.suspended {
                    return Err("Turn lights on before starting capture".into());
                }
                self.stop();
                self.state.error.clear();
                match self.state.settings.mode.as_str() {
                    "screen" => {
                        self.screen = Some(ScreenCapture::start(&self.state.settings.screen_id)?);
                    }
                    "music" => {
                        self.audio =
                            Some(Audio::start(self.state.settings.audio_source == "system")?);
                    }
                    _ => return Err("Select Screen sync or Music before starting capture".into()),
                }
                self.state.capturing = true;
            }
            Action::StopCapture => self.stop(),
            Action::Suspend => {
                // Turn off before releasing capture: an OS capture teardown may
                // wait for the GUI thread that delivered the sleep notification.
                if self.state.settings.off_on_sleep {
                    self.black();
                }
                self.suspended = true;
                self.state.powered = false;
                self.controller.close("Computer sleeping");
                self.screen = None;
                self.audio = None;
                self.state.capturing = false;
                self.state.capture_size = (0, 0);
                self.state.audio_samples = 0;
            }
            Action::Resume => {
                self.suspended = false;
                self.state.error =
                    "Computer awake. Turn lights on; restart capture if needed.".into();
                self.last_solid = None;
            }
            Action::Shutdown => {
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
        self.state.status = self.controller.status.clone();
        Ok(self.state.clone())
    }
    pub fn tick(&mut self) {
        if self.desired
            && !self.suspended
            && !self.controller.status.connected
            && self.retry.elapsed() >= Duration::from_secs(3)
        {
            self.retry = Instant::now();
            let _ = self.controller.connect();
            self.last_solid = None;
        }
        let s = &self.state.settings;
        let generated: Result<Vec<Rgb>, String> = (|| {
            if !self.state.powered || self.suspended {
                return Ok(vec![[0; 3]; s.count]);
            }
            match s.mode.as_str() {
                "solid" => Ok(vec![lighting::hex(&s.color); s.count]),
                "rainbow" => Ok((0..s.count)
                    .map(|i| {
                        lighting::hsv(
                            (i as f32 / s.count as f32
                                + self.started.elapsed().as_secs_f32() / 25.0)
                                % 1.0,
                        )
                    })
                    .collect()),
                "screen" => {
                    if let Some(screen) = self.screen.as_mut() {
                        let colors = screen.colors(s)?;
                        self.state.capture_size = screen.dimensions;
                        Ok(colors)
                    } else {
                        Ok(vec![[0; 3]; s.count])
                    }
                }
                "music" => {
                    if let Some(audio) = self.audio.as_ref() {
                        let (levels, received) = audio.levels(s.count, s.sensitivity)?;
                        self.state.audio_samples = received;
                        let base = lighting::hex(&s.color);
                        Ok(levels
                            .iter()
                            .map(|v| base.map(|c| (c as f32 * v).round() as u8))
                            .collect())
                    } else {
                        Ok(vec![[0; 3]; s.count])
                    }
                }
                _ => Ok(vec![[0; 3]; s.count]),
            }
        })();
        match generated {
            Ok(colors) => {
                self.previous = lighting::transform(&colors, &self.previous, &self.state.settings);
                self.state.colors = self
                    .previous
                    .iter()
                    .map(|rgb| rgb.map(|c| c.round().clamp(0.0, 255.0) as u8))
                    .collect();
            }
            Err(e) => {
                self.stop();
                self.state.error = format!("Capture stopped: {e}");
            }
        }
        if self.controller.status.connected && !self.suspended {
            let s = &self.state.settings;
            let active = self.state.powered && (s.mode == "rainbow" || self.state.capturing);
            let result = if active {
                Some(self.controller.frame(&lighting::reorder(
                    &self.state.colors,
                    s.reverse,
                    s.offset,
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
                self.fps_frames += 1;
                self.state.frames_written += 1;
            }
        }
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
    thread::Builder::new()
        .name("lighting".into())
        .spawn(move || {
            let mut engine = Engine::new(settings, path, warning);
            let mut clock = crate::platform::FrameClock::new();
            let mut next = Instant::now();
            let mut last_event = Instant::now();
            loop {
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
                        !engine.suspended
                            && engine.state.powered
                            && (engine.state.capturing || engine.state.settings.mode == "rainbow"),
                    );
                    let began = Instant::now();
                    engine.tick();
                    next = began + Duration::from_secs_f64(1.0 / engine.state.settings.fps as f64);
                    if last_event.elapsed() >= Duration::from_millis(100) {
                        emit(&engine.state);
                        last_event = Instant::now();
                    }
                }
            }
        })
        .expect("start lighting worker");
    Handle { tx }
}
#[cfg(test)]
mod tests {
    use super::*;
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
        e.action(Action::Suspend).unwrap();
        e.action(Action::Resume).unwrap();
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
}
