use crate::{audio::Feed, engine::Handle};
use std::{
    io::Read,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

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
    thread::Builder::new()
        .name("login1-sleep".into())
        .spawn(move || {
            if let Err(e) = watch_login1(handle) {
                // Non-systemd desktops still support manual lighting and capture.
                eprintln!("Automatic sleep handling unavailable: {e}");
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(())
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
    }
}
