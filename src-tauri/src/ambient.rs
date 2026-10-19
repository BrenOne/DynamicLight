//! Optional lux inputs; native/file IO runs outside the lighting workers.
use serde::{Deserialize, Serialize};
use std::{
    io::Read,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::Manager;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    pub enabled: bool,
    pub source: String,
    pub low_lux: f32,
    pub high_lux: f32,
    pub minimum: u8,
    pub maximum: u8,
    pub fallback: u8,
    pub smoothing: u8,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: false,
            source: "external".into(),
            low_lux: 0.0,
            high_lux: 500.0,
            minimum: 10,
            maximum: 60,
            fallback: 35,
            smoothing: 5,
        }
    }
}
impl Config {
    pub fn validate(&self) -> Result<(), String> {
        let iio = self.source.strip_prefix("iio:device").is_some_and(|s| {
            !s.is_empty() && s.len() <= 8 && s.bytes().all(|b| b.is_ascii_digit())
        });
        if !["external", "windows"].contains(&self.source.as_str()) && !iio {
            return Err("Unknown ambient sensor source".into());
        }
        if !self.low_lux.is_finite()
            || !self.high_lux.is_finite()
            || self.low_lux < 0.0
            || self.high_lux > 100000.0
            || self.low_lux >= self.high_lux
            || self.minimum > self.maximum
            || self.maximum > 100
            || self.fallback > 100
            || !(1..=120).contains(&self.smoothing)
        {
            return Err("Use increasing lux bounds within 0–100000, brightness 0–100%, and smoothing 1–120 seconds".into());
        }
        Ok(())
    }
    fn level(&self, lux: f32) -> f32 {
        let fraction = ((lux - self.low_lux) / (self.high_lux - self.low_lux)).clamp(0.0, 1.0);
        f32::from(self.minimum) + fraction * f32::from(self.maximum - self.minimum)
    }
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub config: Config,
    pub lux: Option<f32>,
    pub available: bool,
    pub manual_override: bool,
    pub brightness: u8,
    pub message: String,
}
pub struct Runtime {
    pub status: Status,
    path: PathBuf,
    last: Option<Instant>,
    level: f32,
}
impl Runtime {
    pub fn load(path: PathBuf) -> Self {
        let config = std::fs::read(&path)
            .ok()
            .filter(|v| v.len() <= 8192)
            .and_then(|v| serde_json::from_slice::<Config>(&v).ok())
            .filter(|v| v.validate().is_ok())
            .unwrap_or_default();
        Self {
            level: f32::from(config.fallback),
            status: Status {
                config,
                lux: None,
                available: false,
                manual_override: false,
                brightness: 0,
                message: "Sensor disabled or awaiting a reading".into(),
            },
            path,
            last: None,
        }
    }
    pub fn configure(&mut self, config: Config, current: u8) -> Result<(), String> {
        config.validate()?;
        crate::settings::save_json(&config, &self.path)?;
        self.status.config = config;
        self.level = f32::from(current);
        self.status.manual_override = false;
        self.status.available = false;
        self.status.lux = None;
        self.last = None;
        Ok(())
    }
    pub fn manual(&mut self) {
        if self.status.config.enabled {
            self.status.manual_override = true;
            self.status.message =
                "Manual brightness override; Resume sensor to re-enable automatic adjustment"
                    .into();
        }
    }
    pub fn resume(&mut self, current: u8) {
        self.level = f32::from(current);
        self.status.manual_override = false;
    }
    pub fn reading(&mut self, source: &str, value: Result<f32, String>) {
        if !self.status.config.enabled || self.status.config.source != source {
            return;
        }
        match value.and_then(valid_lux) {
            Ok(lux) => {
                self.last = Some(Instant::now());
                self.status.lux = Some(lux);
                self.status.available = true;
                if !self.status.manual_override {
                    self.status.message = "Ambient sensor active".into();
                }
            }
            Err(e) => {
                self.status.lux = None;
                self.status.available = false;
                if !self.status.manual_override {
                    self.status.message = format!("Fixed fallback: {e}");
                }
            }
        }
    }
    pub fn brightness(&mut self, manual: u8, dt: f32) -> u8 {
        if !self.status.config.enabled || self.status.manual_override {
            self.status.brightness = manual;
            return manual;
        }
        if self
            .last
            .is_none_or(|t| t.elapsed() > Duration::from_secs(5))
        {
            self.status.available = false;
            self.status.lux = None;
            self.status.message = "No fresh sensor reading; using fixed fallback".into();
        }
        let target = self
            .status
            .lux
            .filter(|_| self.status.available)
            .map(|v| self.status.config.level(v))
            .unwrap_or(f32::from(self.status.config.fallback));
        let alpha = 1.0 - (-dt.max(0.0) / f32::from(self.status.config.smoothing)).exp();
        self.level += (target - self.level) * alpha;
        self.status.brightness = self.level.round().clamp(0.0, 100.0) as u8;
        self.status.brightness
    }
}
fn valid_lux(value: f32) -> Result<f32, String> {
    if value.is_finite() && (0.0..=100000.0).contains(&value) {
        Ok(value)
    } else {
        Err("Sensor returned an invalid lux reading".into())
    }
}
fn external(path: &Path) -> Result<f32, String> {
    let metadata = std::fs::metadata(path).map_err(|_| "External sensor file is unavailable")?;
    if !metadata.is_file() || metadata.len() > 1024 {
        return Err("External input must be a regular JSON file of at most 1 KiB".into());
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Reading {
        lux: f32,
        timestamp: u64,
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|_| "External sensor file is unavailable")?
        .take(1025)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 1024 {
        return Err("External sensor file exceeds 1 KiB".into());
    }
    let r: Reading = serde_json::from_slice(&bytes)
        .map_err(|_| "External sensor file needs lux and timestamp fields")?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    if r.timestamp > now + 5 || now.saturating_sub(r.timestamp) > 30 {
        return Err("External sensor reading is stale or dated in the future".into());
    }
    valid_lux(r.lux)
}
#[cfg(target_os = "windows")]
struct WindowsSensor {
    sensor: Option<windows::Devices::Sensors::LightSensor>,
}
#[cfg(target_os = "windows")]
impl WindowsSensor {
    fn new() -> Result<Self, String> {
        use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};
        unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.map_err(|e| e.to_string())?;
        let result = windows::Devices::Sensors::LightSensor::GetDefault()
            .map_err(|_| "No Windows ambient-light sensor available".to_string());
        match result {
            Ok(sensor) => {
                if let Err(e) = sensor
                    .SetReportInterval(sensor.MinimumReportInterval().unwrap_or(1000).max(1000))
                {
                    drop(sensor);
                    unsafe { windows::Win32::System::WinRT::RoUninitialize() };
                    return Err(e.to_string());
                }
                Ok(Self {
                    sensor: Some(sensor),
                })
            }
            Err(e) => {
                unsafe { windows::Win32::System::WinRT::RoUninitialize() };
                Err(e)
            }
        }
    }
    fn read(&self) -> Result<f32, String> {
        let reading = self
            .sensor
            .as_ref()
            .unwrap()
            .GetCurrentReading()
            .map_err(|_| "Windows sensor has no current reading".to_string())?;
        let stamp = reading
            .Timestamp()
            .map_err(|e| e.to_string())?
            .UniversalTime
            / 10_000_000
            - 11_644_473_600;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        if !(now - 30..=now + 5).contains(&stamp) {
            return Err("Windows sensor reading is stale".into());
        }
        valid_lux(reading.IlluminanceInLux().map_err(|e| e.to_string())?)
    }
}
#[cfg(target_os = "windows")]
impl Drop for WindowsSensor {
    fn drop(&mut self) {
        if let Some(sensor) = self.sensor.take() {
            let _ = sensor.SetReportInterval(0);
            drop(sensor);
        }
        unsafe { windows::Win32::System::WinRT::RoUninitialize() };
    }
}
#[cfg(target_os = "linux")]
fn iio(source: &str) -> Result<f32, String> {
    if !source
        .strip_prefix("iio:device")
        .is_some_and(|s| !s.is_empty() && s.len() <= 8 && s.bytes().all(|b| b.is_ascii_digit()))
    {
        return Err("Invalid IIO source".into());
    }
    let dir = Path::new("/sys/bus/iio/devices").join(source);
    let number = |name: &str| -> Result<f32, String> {
        let mut text = String::new();
        std::fs::File::open(dir.join(name))
            .map_err(|e| e.to_string())?
            .take(128)
            .read_to_string(&mut text)
            .map_err(|e| e.to_string())?;
        text.trim()
            .parse()
            .map_err(|_| "Invalid IIO reading".into())
    };
    if dir.join("in_illuminance_input").exists() {
        valid_lux(number("in_illuminance_input")?)
    } else {
        valid_lux(
            (number("in_illuminance_raw")?
                + if dir.join("in_illuminance_offset").exists() {
                    number("in_illuminance_offset")?
                } else {
                    0.0
                })
                * number("in_illuminance_scale")?,
        )
    }
}
#[derive(Serialize)]
pub struct Source {
    id: String,
    name: String,
    available: bool,
    message: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sources {
    sources: Vec<Source>,
    external_file: String,
}
#[tauri::command]
pub async fn ambient_sources(app: tauri::AppHandle) -> Result<Sources, String> {
    let path = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("ambient-input.json");
    tauri::async_runtime::spawn_blocking(move || {
        let reading = external(&path);
        let mut sources = vec![Source {
            id: "external".into(),
            name: "External JSON lux sensor".into(),
            available: reading.is_ok(),
            message: reading
                .err()
                .unwrap_or_else(|| "Fresh reading available".into()),
        }];
        #[cfg(target_os = "windows")]
        {
            let sensor = WindowsSensor::new();
            sources.push(Source {
                id: "windows".into(),
                name: "Windows ambient-light sensor".into(),
                available: sensor.is_ok(),
                message: sensor.err().unwrap_or_else(|| "OS sensor available".into()),
            });
        }
        #[cfg(target_os = "linux")]
        {
            if let Ok(entries) = std::fs::read_dir("/sys/bus/iio/devices") {
                for entry in entries.flatten().take(128) {
                    let id = entry.file_name().to_string_lossy().into_owned();
                    if !id.starts_with("iio:device")
                        || !(entry.path().join("in_illuminance_input").exists()
                            || entry.path().join("in_illuminance_raw").exists())
                    {
                        continue;
                    }
                    let reading = iio(&id);
                    sources.push(Source {
                        id: id.clone(),
                        name: format!("Linux IIO · {id}"),
                        available: reading.is_ok(),
                        message: reading
                            .err()
                            .unwrap_or_else(|| "OS sensor available".into()),
                    });
                }
            }
        }
        #[cfg(target_os = "macos")]
        {
            sources.push(Source {
                id: "windows".into(),
                name: "Built-in sensor (unavailable on macOS)".into(),
                available: false,
                message: "Use an external JSON lux sensor on macOS".into(),
            });
        }
        Ok(Sources {
            sources,
            external_file: path.to_string_lossy().into_owned(),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn ambient_config(
    app: tauri::AppHandle,
    config: Option<Config>,
) -> Result<Status, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<crate::engine::Handle>()
            .request(crate::engine::Action::AmbientConfig { config })
            .map(|s| s.ambient)
    })
    .await
    .map_err(|e| e.to_string())?
}
pub fn init(app: &tauri::AppHandle) -> Result<(), String> {
    let handle = app.state::<crate::engine::Handle>().inner().clone();
    let path = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("ambient-input.json");
    std::thread::Builder::new()
        .name("ambient-sensors".into())
        .spawn(move || {
            #[cfg(target_os = "windows")]
            let mut windows: Option<WindowsSensor> = None;
            loop {
                std::thread::sleep(Duration::from_secs(1));
                if handle.request(crate::engine::Action::Snapshot).is_err() {
                    break;
                }
                let all = handle.fleet.as_ref().unwrap().all();
                #[cfg(target_os = "windows")]
                let mut windows_needed = false;
                for target in all {
                    let Ok(snapshot) = target.request(crate::engine::Action::Snapshot) else {
                        continue;
                    };
                    let config = &snapshot.ambient.config;
                    if !config.enabled {
                        continue;
                    }
                    let result = match config.source.as_str() {
                        "external" => external(&path),
                        #[cfg(target_os = "windows")]
                        "windows" => {
                            windows_needed = true;
                            if windows.is_none() {
                                windows = WindowsSensor::new().ok();
                            }
                            let result = windows
                                .as_ref()
                                .ok_or_else(|| "Windows ambient-light sensor unavailable".into())
                                .and_then(WindowsSensor::read);
                            if result.is_err() {
                                windows = None;
                            }
                            result
                        }
                        #[cfg(target_os = "linux")]
                        source if source.starts_with("iio:") => iio(source),
                        _ => Err("Sensor source unavailable on this platform".into()),
                    };
                    let _ = target.request(crate::engine::Action::AmbientReading {
                        source: config.source.clone(),
                        result,
                    });
                }
                #[cfg(target_os = "windows")]
                if !windows_needed {
                    windows = None;
                }
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mapping_smoothing_fallback_and_manual_override_are_bounded() {
        let dir = tempfile::tempdir().unwrap();
        let mut runtime = Runtime::load(dir.path().join("ambient.json"));
        let config = Config {
            enabled: true,
            minimum: 10,
            maximum: 70,
            low_lux: 0.0,
            high_lux: 600.0,
            smoothing: 10,
            fallback: 30,
            ..Default::default()
        };
        assert_eq!(config.level(300.0), 40.0);
        assert_eq!(config.level(9999.0), 70.0);
        runtime.configure(config, 20).unwrap();
        runtime.reading("external", Ok(600.0));
        let first = runtime.brightness(80, 1.0);
        assert!((21..30).contains(&first));
        for _ in 0..100 {
            runtime.brightness(80, 1.0);
        }
        assert!((69..=70).contains(&runtime.status.brightness));
        runtime.manual();
        assert_eq!(runtime.brightness(17, 1.0), 17);
        runtime.reading("external", Ok(0.0));
        assert_eq!(runtime.brightness(17, 1.0), 17);
        runtime.resume(17);
        runtime.reading("external", Err("unplugged".into()));
        for _ in 0..100 {
            runtime.brightness(17, 1.0);
        }
        assert_eq!(runtime.status.brightness, 30);
        runtime.reading("different-source", Ok(200.0));
        assert!(!runtime.status.available);
        runtime.reading("external", Ok(f32::NAN));
        assert!(!runtime.status.available);
    }
    #[test]
    fn external_sensor_rejects_stale_future_oversized_and_non_numeric_input() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ambient.json");
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        std::fs::write(&path, format!(r#"{{"lux":123.5,"timestamp":{now}}}"#)).unwrap();
        assert_eq!(external(&path).unwrap(), 123.5);
        for text in [
            format!(r#"{{"lux":12,"timestamp":{}}}"#, now - 60),
            format!(r#"{{"lux":12,"timestamp":{}}}"#, now + 60),
            format!(r#"{{"lux":"unknown","timestamp":{now}}}"#),
            "x".repeat(1025),
        ] {
            std::fs::write(&path, text).unwrap();
            assert!(external(&path).is_err());
        }
    }
}
