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
    sequence: u8,
    pub status: Status,
}
static CLAIMED: std::sync::LazyLock<std::sync::Mutex<std::collections::HashSet<String>>> =
    std::sync::LazyLock::new(Default::default);
impl Drop for Controller {
    fn drop(&mut self) {
        self.close("Closed");
    }
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
    for report in protocol::reports(packet) {
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
            sequence: 0,
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
            self.device = Some(target.open_device(&api).map_err(|e| {
                #[cfg(target_os = "linux")]
                { format!("Cannot open the strip: {e}. Install the DynamicLight USB permission rule, then unplug and reconnect the strip. See docs/linux.md.") }
                #[cfg(not(target_os = "linux"))]
                { e.to_string() }
            })?);
            let id = self.id();
            self.write(&protocol::command(id, 0x82, &[]))?;
            thread::sleep(Duration::from_millis(50));
            let id = self.id();
            self.write(&protocol::command(id, 0x93, &[0]))?;
            let id = self.id();
            self.write(&protocol::command(id, 0x87, &[100]))?;
            self.solid([0; 3], 127)?;
            self.status = Status {
                connected: true,
                message: "Connected over USB".into(),
            };
            Ok(())
        })();
        if let Err(ref error) = result {
            self.close(error);
        }
        result
    }
    fn write(&mut self, packet: &[u8]) -> Result<(), String> {
        let result: Result<(), String> = (|| {
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
        })();
        if let Err(ref e) = result {
            self.close(&format!("USB disconnected: {e}"));
        }
        result
    }
    pub fn solid(&mut self, rgb: Rgb, count: usize) -> Result<(), String> {
        let id = self.id();
        self.write(&protocol::solid(id, rgb, count))
    }
    pub fn frame(&mut self, colors: &[Rgb]) -> Result<(), String> {
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
