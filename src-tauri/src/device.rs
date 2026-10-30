use crate::protocol::{self, Rgb};
use hidapi::{HidApi, HidDevice};
use serde::Serialize;
use std::{thread, time::Duration};

#[derive(Clone, Serialize)]
pub struct Status {
    pub connected: bool,
    pub message: String,
}
pub struct Controller {
    target: String,
    lease: Option<Lease>,
    #[cfg(test)]
    pub test_bus: Option<TestBus>,
    device: Option<HidDevice>,
    path: Option<std::ffi::CString>,
    sequence: u8,
    pub status: Status,
    /// (LED count, display size in inches) reported by the controller (0x82).
    pub detected: Option<(u8, u8)>,
}
static CLAIMED: std::sync::LazyLock<std::sync::Mutex<std::collections::HashSet<String>>> =
    std::sync::LazyLock::new(Default::default);
impl Drop for Controller {
    fn drop(&mut self) {
        self.close("Closed");
    }
}
/// While Windows updates keyboard indicators (Caps/Num/Scroll Lock), the controller's
/// keyboard interface receives a SET_REPORT. If it lands while a frame is being
/// received/driven, the firmware shows garbage (white/random strobes above ~30 FPS).
/// The Windows lock-key hook calls `hold_output` so no frame overlaps that window.
static EPOCH: std::sync::LazyLock<std::time::Instant> =
    std::sync::LazyLock::new(std::time::Instant::now);
static HOLD_UNTIL_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
fn now_ms() -> u64 {
    EPOCH.elapsed().as_millis() as u64
}
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub fn hold_output(ms: u64) {
    HOLD_UNTIL_MS.fetch_max(now_ms() + ms, std::sync::atomic::Ordering::Relaxed);
}
fn held_for() -> Option<Duration> {
    let until = HOLD_UNTIL_MS.load(std::sync::atomic::Ordering::Relaxed);
    let now = now_ms();
    (until > now).then(|| Duration::from_millis(until - now))
}
struct Lease(String);
impl Lease {
    fn claim(key: String) -> Result<Self, String> {
        if !CLAIMED.lock().unwrap().insert(key.clone()) {
            Err("This USB interface already belongs to another controller. Check routing.".into())
        } else {
            Ok(Self(key))
        }
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        CLAIMED.lock().unwrap().remove(&self.0);
    }
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Choice {
    #[serde(skip)]
    pub aliases: Vec<String>,
    pub id: String,
    pub name: String,
    pub serial: bool,
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn supported(d: &hidapi::DeviceInfo) -> bool {
    d.vendor_id() == protocol::VID
        && d.product_id() == protocol::PID
        && lighting_interface(
            d.interface_number(),
            d.usage_page(),
            &d.path().to_string_lossy(),
        )
}
fn identity(d: &hidapi::DeviceInfo, all: &[&hidapi::DeviceInfo]) -> String {
    if let Some(serial) = d.serial_number().filter(|s| !s.is_empty()) {
        if all
            .iter()
            .filter(|x| x.serial_number() == Some(serial))
            .count()
            == 1
        {
            return format!("s-{}", hex(serial.as_bytes()));
        }
    }
    format!("p-{}", hex(d.path().to_bytes()))
}
fn aliases(preferred: &str, path: &[u8]) -> Vec<String> {
    let local = format!("p-{}", hex(path));
    if local == preferred {
        vec![local]
    } else {
        vec![local, preferred.into()]
    }
}
pub fn choices() -> Result<Vec<Choice>, String> {
    let api = HidApi::new().map_err(|e| e.to_string())?;
    let all: Vec<_> = api.device_list().filter(|d| supported(d)).collect();
    Ok(all
        .iter()
        .enumerate()
        .take(32)
        .map(|(i, d)| {
            let id = identity(d, &all);
            Choice {
                aliases: aliases(&id, d.path().to_bytes()),
                serial: id.starts_with("s-"),
                id,
                name: format!(
                    "{} · {}",
                    d.product_string().unwrap_or("ROBOBLOQ strip"),
                    d.serial_number()
                        .filter(|s| !s.is_empty())
                        .map(|s| s.chars().take(40).collect())
                        .unwrap_or_else(|| format!("USB interface {}", i + 1))
                ),
            }
        })
        .collect())
}
#[cfg(test)]
pub struct TestBus {
    pub online: bool,
    pub connects: usize,
    pub writes: usize,
    pub last_packet: Vec<u8>,
}
pub fn lighting_interface(interface: i32, usage_page: u16, path: &str) -> bool {
    // Reject the known keyboard interface even if its path/usage is unusual.
    interface != 1
        && usage_page != 1
        && (interface == 0 || path.to_ascii_lowercase().contains("mi_00") || usage_page >= 0xff00)
}
fn write_reports(
    packet: &[u8],
    mut write: impl FnMut(&[u8]) -> Result<usize, String>,
) -> Result<(), String> {
    for (i, report) in protocol::reports(packet).enumerate() {
        if i > 0 {
            // The controller drops/stalls back-to-back reports (OpenRGB uses the same gap).
            thread::sleep(Duration::from_millis(1));
        }
        let n = write(&report)?;
        if n != report.len() {
            return Err(format!("Incomplete USB write ({n}/{})", report.len()));
        }
    }
    Ok(())
}
impl Controller {
    pub fn new() -> Self {
        Self {
            target: String::new(),
            lease: None,
            #[cfg(test)]
            test_bus: None,
            device: None,
            path: None,
            sequence: 0,
            detected: None,
            status: Status {
                connected: false,
                message: "Ready to connect".into(),
            },
        }
    }
    fn id(&mut self) -> u8 {
        self.sequence = self.sequence % 254 + 1;
        self.sequence
    }
    pub fn close(&mut self, message: &str) {
        self.device = None;
        self.path = None;
        self.lease = None;
        self.status = Status {
            connected: false,
            message: message.into(),
        };
    }
    pub fn bind(&mut self, target: String) {
        if self.target != target {
            self.close("Controller binding changed");
            self.target = target;
        }
    }
    pub fn connect(&mut self) -> Result<(), String> {
        self.close("Connecting");
        let result: Result<(), String> = (|| {
            #[cfg(test)]
            if let Some(bus) = &mut self.test_bus {
                bus.connects += 1;
                if !bus.online {
                    return Err("Test USB unavailable".into());
                }
                self.status = Status {
                    connected: true,
                    message: "Test USB".into(),
                };
                return Ok(());
            }
            let api = HidApi::new().map_err(|e| e.to_string())?;
            let all: Vec<_> = api.device_list().filter(|d| supported(d)).collect();
            if self.target.is_empty() && all.len() > 1 {
                return Err(
                    "Several strips found. Bind this controller in Device setup before starting."
                        .into(),
                );
            }
            let target = all
                .iter()
                .copied()
                .find(|d| {
                    self.target.is_empty()
                        || aliases(&identity(d, &all), d.path().to_bytes()).contains(&self.target)
                })
                .ok_or("No compatible strip found. Check USB and close SyncLight.")?;
            let key = hex(target.path().to_bytes());
            self.lease = Some(Lease::claim(key)?);
            self.path = Some(target.path().to_owned());
            self.device = Some(target.open_device(&api).map_err(|e| {
                #[cfg(target_os = "linux")]
                { format!("Cannot open the strip: {e}. Install the DynamicLight USB permission rule, then unplug and reconnect the strip. See docs/linux.md.") }
                #[cfg(not(target_os = "linux"))]
                { e.to_string() }
            })?);
            self.detected = self.query_info();
            // Stop the controller from typing the driver-download URL.
            let id = self.id();
            self.write(&protocol::command(id, 0x93, &[0]))?;
            // Hardware brightness is 0-255; software brightness scales colours.
            let id = self.id();
            self.write(&protocol::command(id, 0x87, &[255]))?;
            self.solid([0; 3], 127)?;
            self.status = Status {
                connected: true,
                message: match self.detected {
                    Some((leds, size)) => format!("Connected over USB · {leds} LEDs · {size}\""),
                    None => "Connected over USB".into(),
                },
            };
            Ok(())
        })();
        if let Err(ref error) = result {
            self.close(error);
        }
        result
    }
    /// Asks for device info (0x82). Reply: 52 42 len id 82 .. [8]=inches .. [11]=LEDs.
    fn query_info(&mut self) -> Option<(u8, u8)> {
        #[cfg(test)]
        if self.test_bus.is_some() {
            return None;
        }
        for _ in 0..3 {
            let id = self.id();
            let packet = protocol::command(id, 0x82, &[]);
            let d = self.device.as_ref()?;
            write_reports(&packet, |r| d.write(r).map_err(|e| e.to_string())).ok()?;
            let mut buf = [0u8; 65];
            for _ in 0..5 {
                let n = d.read_timeout(&mut buf, 100).ok()?;
                if n == 0 {
                    continue;
                }
                // Tolerate a leading report-ID byte on some platforms.
                let o = if buf[0] == 0 { 1 } else { 0 };
                if n >= o + 24 && buf[o] == 0x52 && buf[o + 1] == 0x42 && buf[o + 4] == 0x82 {
                    let (leds, inches) = (buf[o + 11], buf[o + 8]);
                    return (1..=127).contains(&leds).then_some((leds, inches));
                }
            }
            // The controller sometimes holds a reply until it receives another command.
            let id = self.id();
            let nop = protocol::command(id, 0x93, &[0]);
            let d = self.device.as_ref()?;
            let _ = write_reports(&nop, |r| d.write(r).map_err(|e| e.to_string()));
        }
        None
    }
    /// Reopen the same interface after a stalled write without a full reconnect.
    fn reopen(&mut self) -> Result<(), String> {
        let path = self.path.clone().ok_or("Strip is not connected")?;
        self.device = None;
        let api = HidApi::new().map_err(|e| e.to_string())?;
        self.device = Some(api.open_path(&path).map_err(|e| e.to_string())?);
        Ok(())
    }
    fn write(&mut self, packet: &[u8]) -> Result<(), String> {
        let result = self.write_once(packet).or_else(|first| {
            #[cfg(test)]
            if self.test_bus.is_some() {
                return Err(first);
            }
            // A single hid_write timeout (0x3E5) is usually transient: retry, then reopen.
            thread::sleep(Duration::from_millis(5));
            if self.write_once(packet).is_ok() {
                return Ok(());
            }
            self.reopen().map_err(|e| format!("{first}; reopen: {e}"))?;
            self.write_once(packet)
                .map_err(|e| format!("{first}; retry: {e}"))
        });
        if let Err(ref e) = result {
            self.close(&format!("USB disconnected: {e}"));
        }
        result
    }
    fn write_once(&mut self, packet: &[u8]) -> Result<(), String> {
        (|| {
            #[cfg(test)]
            if let Some(bus) = &mut self.test_bus {
                bus.writes += 1;
                bus.last_packet = packet.to_vec();
                return write_reports(packet, |r| {
                    if bus.online {
                        Ok(r.len())
                    } else {
                        Err("Test USB unplugged".into())
                    }
                });
            }
            let d = self.device.as_ref().ok_or("Strip is not connected")?;
            write_reports(packet, |r| d.write(r).map_err(|e| e.to_string()))?;
            // Drain replies without blocking or allowing an endless read loop.
            let mut buf = [0; 65];
            for _ in 0..8 {
                if d.read_timeout(&mut buf, 0).map_err(|e| e.to_string())? == 0 {
                    break;
                }
            }
            Ok(())
        })()
    }
    pub fn solid(&mut self, rgb: Rgb, count: usize) -> Result<(), String> {
        // Solid colours are sent once, so wait out an indicator update instead of
        // skipping it (re-checked: a held key keeps extending the window), but never
        // stall the engine for more than a second.
        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        while let Some(wait) = held_for() {
            if std::time::Instant::now() >= deadline {
                break;
            }
            thread::sleep(wait.min(Duration::from_millis(50)));
        }
        let id = self.id();
        self.write(&protocol::solid(id, rgb, count))
    }
    pub fn frame(&mut self, colors: &[Rgb]) -> Result<(), String> {
        // Frames stream continuously; drop the ones that would collide with a lock key.
        if held_for().is_some() {
            return Ok(());
        }
        let id = self.id();
        self.write(&protocol::frame(id, colors))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn claims_are_exclusive_and_released_on_drop() {
        let first = Lease::claim("fixture-interface".into()).unwrap();
        assert!(Lease::claim("fixture-interface".into()).is_err());
        let other = Lease::claim("other-fixture-interface".into()).unwrap();
        drop(first);
        assert!(Lease::claim("fixture-interface".into()).is_ok());
        drop(other);
    }
    #[test]
    fn path_bindings_remain_valid_when_another_duplicate_serial_disconnects() {
        let before = aliases("p-6162", b"ab");
        let after = aliases("s-3132", b"ab");
        assert!(before.contains(&"p-6162".into()));
        assert!(after.contains(&"p-6162".into()));
        assert!(after.contains(&"s-3132".into()));
    }
    #[test]
    fn never_open_keyboard() {
        assert!(lighting_interface(0, 0xff00, "x"));
        assert!(lighting_interface(-1, 0xff00, "x"));
        assert!(!lighting_interface(1, 0xff00, "mi_00"));
        assert!(!lighting_interface(0, 1, "mi_00"));
        assert!(!lighting_interface(-1, 1, "x"));
    }
    #[test]
    fn incomplete_write_stops_remaining_reports() {
        let packet = protocol::frame(1, &[[0, 0, 0]; 65]);
        let mut calls = 0;
        assert!(write_reports(&packet, |_| {
            calls += 1;
            Ok(if calls == 2 { 64 } else { 65 })
        })
        .is_err());
        assert_eq!(calls, 2);
    }
    #[test]
    fn failed_write_does_not_queue_more_reports() {
        let mut calls = 0;
        assert!(write_reports(&[0; 200], |_| {
            calls += 1;
            Err("unplugged".into())
        })
        .is_err());
        assert_eq!(calls, 1);
    }
}
