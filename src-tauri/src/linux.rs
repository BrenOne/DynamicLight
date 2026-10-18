use crate::{audio::Feed, engine::Handle, lifecycle::SystemEvent};
use std::{
    io::Read,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::Duration,
};
static AUDIO_WATCH: Mutex<Option<Child>> = Mutex::new(None);
static WATCH_STOP: AtomicBool = AtomicBool::new(false);

pub fn stop_watchers() {
    WATCH_STOP.store(true, Ordering::SeqCst);
    if let Ok(mut slot) = AUDIO_WATCH.lock() {
        if let Some(mut child) = slot.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

pub fn wayland() -> bool {
    std::env::var("XDG_SESSION_TYPE").is_ok_and(|v| v.eq_ignore_ascii_case("wayland"))
        || std::env::var_os("WAYLAND_DISPLAY").is_some_and(|v| !v.is_empty())
}

// PulseAudio's monitor source also works with PipeWire's PulseAudio service.
// Keep the reader off the lighting worker; kill + reap before joining on stop.
pub struct SystemAudio {
    child: Child,
    reader: Option<JoinHandle<()>>,
    stopping: Arc<AtomicBool>,
}
impl SystemAudio {
    pub fn start(feed: Feed) -> Result<Self, String> {
        let mut child = Command::new("parec")
            .args(["--device=@DEFAULT_MONITOR@", "--format=float32le", "--rate=48000",
                "--channels=1", "--raw", "--latency-msec=20", "--client-name=DynamicLight"])
            .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null())
            .spawn().map_err(|e| format!("Computer audio needs parec (pulseaudio-utils) and a running PulseAudio or pipewire-pulse service: {e}"))?;
        let mut output = child.stdout.take().ok_or("Audio pipe unavailable")?;
        let stopping = Arc::new(AtomicBool::new(false));
        let stop = stopping.clone();
        let (ready, started) = mpsc::sync_channel(1);
        let reader = thread::Builder::new().name("pulse-monitor".into()).spawn(move || {
            let mut bytes = [0u8; 1024];
            let mut first = true;
            while output.read_exact(&mut bytes).is_ok() {
                if stop.load(Ordering::Relaxed) { break; }
                if let Ok(mut samples) = feed.lock() {
                    samples.push(bytes.as_chunks::<4>().0.iter().map(|b| f32::from_le_bytes(*b)));
                }
                if first { let _ = ready.send(()); first = false; }
            }
            if !stop.load(Ordering::Relaxed) {
                if let Ok(mut samples) = feed.lock() {
                    samples.error = Some("Computer audio stopped. Check PulseAudio or pipewire-pulse, then start capture again.".into());
                }
            }
        });
        let reader = match reader {
            Ok(reader) => reader,
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(e.to_string());
            }
        };
        let stream = Self {
            child,
            reader: Some(reader),
            stopping,
        };
        started.recv_timeout(Duration::from_secs(3))
            .map_err(|_| "No computer audio received. Check that PulseAudio or pipewire-pulse is running and has a default output device.".to_string())?;
        Ok(stream)
    }
}
impl Drop for SystemAudio {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Relaxed);
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

pub fn watch_power(handle: Handle) -> Result<(), String> {
    let session_handle = handle.clone();
    thread::Builder::new()
        .name("desktop-state".into())
        .spawn(move || {
            if let Err(e) = watch_desktop(&session_handle) {
                session_handle.system_event(
                    SystemEvent::Warning(format!("Display/lock monitoring unavailable: {e}")),
                    false,
                );
                session_handle.system_event(SystemEvent::Ready, false);
            }
        })
        .map_err(|e| e.to_string())?;
    let audio_handle = handle.clone();
    thread::Builder::new()
        .name("audio-devices".into())
        .spawn(move || watch_audio_devices(audio_handle))
        .map_err(|e| e.to_string())?;
    thread::Builder::new()
        .name("login1-sleep".into())
        .spawn(move || {
            if let Err(e) = watch_login1(handle.clone()) {
                // Non-systemd desktops still support manual lighting and capture.
                handle.system_event(
                    SystemEvent::Warning(format!("Automatic sleep handling unavailable: {e}")),
                    false,
                );
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}
fn watch_audio_devices(handle: Handle) {
    use std::io::{BufRead, BufReader};
    while !WATCH_STOP.load(Ordering::SeqCst) {
        let child = Command::new("pactl")
            .arg("subscribe")
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();
        if let Ok(mut child) = child {
            let stdout = child.stdout.take();
            {
                let Ok(mut slot) = AUDIO_WATCH.lock() else {
                    let _ = child.kill();
                    let _ = child.wait();
                    return;
                };
                if WATCH_STOP.load(Ordering::SeqCst) {
                    let _ = child.kill();
                    let _ = child.wait();
                    return;
                }
                *slot = Some(child);
            }
            if let Some(stdout) = stdout {
                for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                    // Server changes include the default sink. Ignore sink-input events
                    // generated by our own stream and volume changes on individual sinks.
                    if line.contains("on server") {
                        handle.system_event(SystemEvent::AudioChanged, false);
                    }
                }
            }
            if let Ok(mut slot) = AUDIO_WATCH.lock() {
                if let Some(mut child) = slot.take() {
                    let _ = child.kill();
                    let _ = child.wait();
                }
            }
        }
        if !WATCH_STOP.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_secs(5));
        }
    }
}
fn watch_desktop(handle: &Handle) -> Result<(), Box<dyn std::error::Error>> {
    use x11rb::protocol::dpms::{ConnectionExt, DPMSMode};
    use zbus::{
        blocking::{Connection, Proxy},
        zvariant::OwnedObjectPath,
    };
    let bus = Connection::system().ok();
    let session = bus.as_ref().and_then(|bus| {
        let manager = Proxy::new(
            bus,
            "org.freedesktop.login1",
            "/org/freedesktop/login1",
            "org.freedesktop.login1.Manager",
        )
        .ok()?;
        let path: OwnedObjectPath = manager
            .call("GetSessionByPID", &(std::process::id(),))
            .ok()?;
        Proxy::new(
            bus,
            "org.freedesktop.login1",
            path,
            "org.freedesktop.login1.Session",
        )
        .ok()
    });
    if session.is_none() {
        handle.system_event(
            SystemEvent::Warning("Lock detection needs a logind desktop session.".into()),
            false,
        );
    }
    let display = if wayland() {
        handle.system_event(SystemEvent::Warning("Display-only sleep detection needs X11 on Linux. Lock and system sleep still work where logind is available.".into()), false);
        None
    } else {
        let connection = x11rb::connect(None).ok().map(|(connection, _)| connection);
        if connection.is_none() {
            handle.system_event(
                SystemEvent::Warning(
                    "X11 display sleep detection is unavailable on this desktop.".into(),
                ),
                false,
            );
        }
        connection
    };
    let mut last_lock = None;
    let mut last_display = None;
    let mut ready = false;
    loop {
        if let Some(session) = &session {
            match session.get_property::<bool>("LockedHint") {
                Ok(locked) if last_lock != Some(locked) => {
                    handle.system_event(SystemEvent::Locked(locked), false);
                    last_lock = Some(locked);
                }
                Err(_) => handle.system_event(
                    SystemEvent::Warning("Desktop lock state is unavailable; check logind.".into()),
                    false,
                ),
                _ => {}
            }
        }
        if let Some(connection) = &display {
            if let Some(reply) = connection
                .dpms_info()
                .ok()
                .and_then(|cookie| cookie.reply().ok())
            {
                let off = reply.state && reply.power_level != DPMSMode::ON;
                if last_display != Some(off) {
                    handle.system_event(SystemEvent::DisplayOff(off), false);
                    last_display = Some(off);
                }
            } else if last_display.is_none() {
                handle.system_event(
                    SystemEvent::Warning(
                        "X11 display sleep detection is unavailable on this desktop.".into(),
                    ),
                    false,
                );
                last_display = Some(false);
            }
        }
        if !ready {
            handle.system_event(SystemEvent::Ready, false);
            ready = true;
        }
        thread::sleep(Duration::from_secs(2));
    }
}
fn watch_login1(handle: Handle) -> Result<(), Box<dyn std::error::Error>> {
    use zbus::{
        blocking::{Connection, Proxy},
        zvariant::OwnedFd,
    };
    let connection = Connection::system()?;
    let proxy = Proxy::new(
        &connection,
        "org.freedesktop.login1",
        "/org/freedesktop/login1",
        "org.freedesktop.login1.Manager",
    )?;
    let mut signals = proxy.receive_signal("PrepareForSleep")?;
    let inhibit = || {
        proxy.call::<_, _, OwnedFd>(
            "Inhibit",
            &("sleep", "DynamicLight", "Turn off the LED strip", "delay"),
        )
    };
    let mut inhibitor = inhibit().ok();
    for signal in &mut signals {
        let (sleeping,): (bool,) = signal.body().deserialize()?;
        handle.power_event(sleeping);
        if sleeping {
            inhibitor.take();
        } else {
            inhibitor = inhibit().ok();
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires a PulseAudio/PipeWire server; CI supplies a null sink"]
    fn pulse_monitor_receives_samples_and_stops() {
        let feed = Arc::new(std::sync::Mutex::new(crate::audio::Samples::new()));
        let stream = SystemAudio::start(feed.clone()).unwrap();
        let pid = stream.child.id();
        assert!(feed.lock().unwrap().received >= 256);
        drop(stream);
        assert!(
            !std::path::Path::new(&format!("/proc/{pid}")).exists(),
            "audio process must be reaped"
        );
        assert!(
            feed.lock().unwrap().error.is_none(),
            "intentional stop is not an error"
        );
        let dir = tempfile::tempdir().unwrap();
        let handle = crate::engine::start(
            crate::settings::Settings::default(),
            dir.path().join("settings.json"),
            String::new(),
            |_| {},
        );
        let watcher_handle = handle.clone();
        let watcher = thread::spawn(move || watch_audio_devices(watcher_handle));
        let mut watcher_pid = None;
        for _ in 0..100 {
            watcher_pid = AUDIO_WATCH.lock().unwrap().as_ref().map(Child::id);
            if watcher_pid.is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        let watcher_pid = watcher_pid.expect("pactl subscribe started");
        stop_watchers();
        watcher.join().unwrap();
        assert!(
            !std::path::Path::new(&format!("/proc/{watcher_pid}")).exists(),
            "device watcher must be reaped on exit"
        );
        handle.request(crate::engine::Action::Shutdown).unwrap();
    }
}
