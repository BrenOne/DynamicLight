//! Versioned, data-only scene authoring. No scripts or unbounded frame processing.
use crate::{lighting, protocol::Rgb};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Mutex};
use tauri::Manager;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Stop {
    pub position: u16,
    pub color: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Block {
    Gradient,
    Wave,
    Breath,
    Sequence,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Layer {
    pub kind: Block,
    pub opacity: u8,
    pub stops: Vec<Stop>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Composition {
    pub version: u8,
    pub duration_ms: u32,
    /// Zero repeats forever; otherwise hold the last frame after these cycles.
    pub repeats: u16,
    pub layers: Vec<Layer>,
}
impl Default for Composition {
    fn default() -> Self {
        Self {
            version: 1,
            duration_ms: 6000,
            repeats: 0,
            layers: vec![Layer {
                kind: Block::Wave,
                opacity: 100,
                stops: vec![
                    Stop {
                        position: 0,
                        color: "#7046ff".into(),
                    },
                    Stop {
                        position: 1000,
                        color: "#20dbaa".into(),
                    },
                ],
            }],
        }
    }
}
impl Composition {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1
            || !(100..=3_600_000).contains(&self.duration_ms)
            || self.repeats > 10000
            || self.layers.is_empty()
            || self.layers.len() > 4
        {
            return Err(
                "Effect v1 needs 1–4 layers, 0.1–3600 second duration and 0–10000 repeats".into(),
            );
        }
        for layer in &self.layers {
            if layer.opacity > 100
                || !(2..=16).contains(&layer.stops.len())
                || layer.stops.first().is_none_or(|s| s.position != 0)
                || layer.stops.last().is_none_or(|s| s.position != 1000)
                || layer
                    .stops
                    .windows(2)
                    .any(|s| s[0].position >= s[1].position)
                || layer
                    .stops
                    .iter()
                    .any(|s| !crate::sharing::valid_color(&s.color))
            {
                return Err(
                    "Each layer needs 2–16 ordered color stops from 0 to 1000 and 0–100% opacity"
                        .into(),
                );
            }
        }
        Ok(())
    }
    pub fn color(&self, position: f32, seconds: f32, speed: u8, reverse: bool) -> Rgb {
        let elapsed = seconds.max(0.0) * speed as f32 / 50.0 * 1000.0 / self.duration_ms as f32;
        let phase = if self.repeats > 0 && elapsed >= self.repeats as f32 {
            1.0
        } else {
            elapsed.fract()
        };
        let x = if reverse { 1.0 - position } else { position };
        let mut result = [0.0; 3];
        for layer in &self.layers {
            let location = match layer.kind {
                Block::Gradient | Block::Breath => x,
                Block::Wave => (x - phase).rem_euclid(1.0),
                Block::Sequence => phase,
            } * 1000.0;
            let pair = layer
                .stops
                .windows(2)
                .find(|s| location <= s[1].position as f32)
                .unwrap_or(&layer.stops[layer.stops.len() - 2..]);
            let t = ((location - pair[0].position as f32)
                / (pair[1].position - pair[0].position) as f32)
                .clamp(0.0, 1.0);
            let a = lighting::hex(&pair[0].color);
            let b = lighting::hex(&pair[1].color);
            let level = if matches!(layer.kind, Block::Breath) {
                0.5 - 0.5 * (phase * std::f32::consts::TAU).cos()
            } else {
                1.0
            };
            let alpha = layer.opacity as f32 / 100.0;
            for c in 0..3 {
                result[c] = result[c] * (1.0 - alpha)
                    + (a[c] as f32 * (1.0 - t) + b[c] as f32 * t) * level * alpha;
            }
        }
        result.map(|v| v.round().clamp(0.0, 255.0) as u8)
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Zone {
    pub name: String,
    /// Fraction of the calibrated route, in thousandths. Last overlapping zone wins.
    pub start: u16,
    pub end: u16,
    pub color: String,
    pub effect: Option<Composition>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Bias {
    pub enabled: bool,
    pub kelvin: u16,
    pub gains: [u16; 3],
    pub measurement: String,
}
impl Default for Bias {
    fn default() -> Self {
        Self {
            enabled: false,
            kelvin: 6500,
            gains: [100; 3],
            measurement: String::new(),
        }
    }
}
impl Bias {
    pub fn validate(&self) -> Result<(), String> {
        if !(2700..=10000).contains(&self.kelvin)
            || self.gains.iter().any(|v| *v > 200)
            || self.measurement.chars().count() > 200
            || self.measurement.chars().any(char::is_control)
        {
            return Err("Bias calibration needs 2700–10000 K, gains 0–200%, and a measurement note up to 200 characters".into());
        }
        Ok(())
    }
    pub fn color(&self) -> Rgb {
        // Reference swatches only: this is not a spectral model or a measured white point.
        let stops = [
            (2700, [255.0, 167.0, 87.0]),
            (4000, [255.0, 206.0, 166.0]),
            (6500, [255.0, 249.0, 253.0]),
            (10000, [202.0, 218.0, 255.0]),
        ];
        let p = stops.windows(2).find(|p| self.kelvin <= p[1].0).unwrap();
        let t = (self.kelvin - p[0].0) as f32 / (p[1].0 - p[0].0) as f32;
        std::array::from_fn(|c| {
            ((p[0].1[c] * (1.0 - t) + p[1].1[c] * t) * self.gains[c] as f32 / 100.0)
                .round()
                .clamp(0.0, 255.0) as u8
        })
    }
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Studio {
    pub effect: Composition,
    pub zones: Vec<Zone>,
    pub bias: Bias,
}
fn valid_name(name: &str) -> bool {
    !name.trim().is_empty() && name.chars().count() <= 50 && !name.chars().any(char::is_control)
}
impl Studio {
    pub fn validate(&self) -> Result<(), String> {
        self.effect.validate()?;
        self.bias.validate()?;
        if self.zones.len() > 16 {
            return Err("Use at most 16 zones".into());
        }
        for zone in &self.zones {
            if !valid_name(&zone.name)
                || zone.start >= zone.end
                || zone.end > 1000
                || !crate::sharing::valid_color(&zone.color)
            {
                return Err(
                    "Zones need a name, color and nonempty range within the calibrated strip"
                        .into(),
                );
            }
            if let Some(effect) = &zone.effect {
                effect.validate()?;
            }
        }
        Ok(())
    }
    pub fn overlay(&self, colors: &mut [Rgb], seconds: f32, speed: u8, reverse: bool) {
        let count = colors.len();
        for (i, color) in colors.iter_mut().enumerate() {
            let p = (i as f32 + 0.5) / count as f32 * 1000.0;
            for zone in &self.zones {
                if p >= zone.start as f32 && p < zone.end as f32 {
                    *color = zone.effect.as_ref().map_or_else(
                        || lighting::hex(&zone.color),
                        |e| {
                            e.color(
                                (p - zone.start as f32) / (zone.end - zone.start) as f32,
                                seconds,
                                speed,
                                reverse,
                            )
                        },
                    );
                }
            }
        }
    }
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectPreset {
    pub name: String,
    pub effect: Composition,
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BiasPreset {
    pub name: String,
    pub bias: Bias,
    pub brightness: u8,
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Library {
    pub version: u8,
    pub effects: Vec<EffectPreset>,
    pub bias: Vec<BiasPreset>,
}
impl Default for Library {
    fn default() -> Self {
        Self {
            version: 1,
            effects: vec![],
            bias: vec![],
        }
    }
}
impl Library {
    fn validate(&self) -> Result<(), String> {
        if self.version != 1 || self.effects.len() > 40 || self.bias.len() > 40 {
            return Err("Studio v1 supports up to 40 presets of each kind".into());
        }
        let mut names = std::collections::HashSet::new();
        for p in &self.effects {
            if !valid_name(&p.name) || !names.insert(&p.name) {
                return Err("Use unique effect names of 1–50 characters".into());
            }
            p.effect.validate()?;
        }
        names.clear();
        for p in &self.bias {
            if !valid_name(&p.name) || !names.insert(&p.name) || p.brightness > 100 {
                return Err("Use unique calibration names and brightness 0–100".into());
            }
            p.bias.validate()?;
        }
        Ok(())
    }
}
struct Store {
    library: Mutex<Library>,
    path: PathBuf,
}
pub fn init(app: &tauri::AppHandle) -> Result<(), String> {
    let path = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("studio.json");
    let library = std::fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Library>(&bytes).ok())
        .filter(|l| l.validate().is_ok())
        .unwrap_or_default();
    app.manage(Store {
        library: Mutex::new(library),
        path,
    });
    Ok(())
}
#[tauri::command]
pub fn studio_library(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    next: Option<Library>,
    expected: Option<Library>,
) -> Result<Library, String> {
    if window.label() != "main" {
        return Err("Open the studio to edit presets".into());
    }
    let store = app.state::<Store>();
    let mut library = store.library.lock().map_err(|e| e.to_string())?;
    if let Some(next) = next {
        next.validate()?;
        if expected.as_ref() != Some(&*library) {
            return Err("Presets changed; reopen the editor".into());
        }
        crate::settings::save_json(&next, &store.path)?;
        *library = next;
    }
    Ok(library.clone())
}
#[tauri::command]
pub fn studio_preview(
    settings: crate::settings::Settings,
    seconds: f32,
) -> Result<Vec<Rgb>, String> {
    settings.validate()?;
    if !seconds.is_finite() || !(0.0..=86400.0).contains(&seconds) {
        return Err("Invalid preview time".into());
    }
    let mut colors = if settings.mode == "rainbow" {
        crate::effects::animated(&settings, seconds)
    } else {
        vec![
            if settings.studio.bias.enabled {
                settings.studio.bias.color()
            } else {
                lighting::hex(&settings.color)
            };
            settings.count
        ]
    };
    settings.studio.overlay(
        &mut colors,
        seconds,
        settings.speed,
        settings.effect_reverse,
    );
    let frames = lighting::transform(&colors, &[], &settings, 1.0 / 30.0);
    Ok(frames.iter().map(|c| c.map(|v| v.round() as u8)).collect())
}

#[tauri::command]
pub async fn wallpaper_image(window: tauri::WebviewWindow) -> Result<Vec<u8>, String> {
    if window.label() != "main" {
        return Err("Open the studio to import wallpaper colors".into());
    }
    tauri::async_runtime::spawn_blocking(wallpaper_bytes)
        .await
        .map_err(|e| e.to_string())?
}
#[cfg(not(target_os = "windows"))]
fn wallpaper_bytes() -> Result<Vec<u8>, String> {
    Err("Automatic wallpaper import is available on Windows. Choose your wallpaper image file on this platform.".into())
}
#[cfg(target_os = "windows")]
fn wallpaper_bytes() -> Result<Vec<u8>, String> {
    use std::io::Read;
    use windows::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, SPI_GETDESKWALLPAPER, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    };
    let mut path = [0u16; 260];
    unsafe {
        SystemParametersInfoW(
            SPI_GETDESKWALLPAPER,
            path.len() as u32,
            Some(path.as_mut_ptr().cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    }
    .map_err(|e| e.to_string())?;
    let path =
        String::from_utf16_lossy(&path[..path.iter().position(|c| *c == 0).unwrap_or(path.len())]);
    if path.is_empty() {
        return Err("No static wallpaper is configured. Choose an image file.".into());
    }
    // Only the OS-selected file can be read; callers cannot supply an arbitrary path.
    let file = std::fs::File::open(path)
        .map_err(|_| "Wallpaper image is unavailable; choose an image file")?;
    let mut bytes = Vec::new();
    file.take(20 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Unable to read wallpaper")?;
    if bytes.len() > 20 * 1024 * 1024 {
        return Err("Choose a wallpaper image smaller than 20 MiB".into());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_unbounded_and_invalid_effects() {
        let mut effect = Composition::default();
        effect.validate().unwrap();
        effect.layers[0].stops[1].position = 0;
        assert!(effect.validate().is_err());
        effect = Composition::default();
        effect.duration_ms = 0;
        assert!(effect.validate().is_err());
        effect = Composition::default();
        effect.layers = vec![effect.layers[0].clone(); 5];
        assert!(effect.validate().is_err());
    }
    #[test]
    fn sequence_stops_repeats_and_zone_order_are_deterministic() {
        let mut effect = Composition::default();
        effect.layers[0].kind = Block::Sequence;
        effect.layers[0].stops = vec![
            Stop {
                position: 0,
                color: "#ff0000".into(),
            },
            Stop {
                position: 1000,
                color: "#0000ff".into(),
            },
        ];
        effect.duration_ms = 1000;
        effect.repeats = 1;
        assert_eq!(effect.color(0.0, 0.5, 50, false), [128, 0, 128]);
        assert_eq!(effect.color(0.0, 100.0, 50, false), [0, 0, 255]);
        let studio = Studio {
            zones: vec![
                Zone {
                    name: "Full".into(),
                    start: 0,
                    end: 1000,
                    color: "#ff0000".into(),
                    effect: None,
                },
                Zone {
                    name: "Right".into(),
                    start: 500,
                    end: 1000,
                    color: "#0000ff".into(),
                    effect: None,
                },
            ],
            ..Default::default()
        };
        let mut colors = [[0; 3]; 4];
        studio.overlay(&mut colors, 0.0, 50, false);
        assert_eq!(colors, [[255, 0, 0], [255, 0, 0], [0, 0, 255], [0, 0, 255]]);
    }
    #[test]
    fn zone_output_obeys_global_comfort_caps() {
        let mut settings = crate::settings::Settings {
            brightness: 100,
            max_brightness: 10,
            ..Default::default()
        };
        settings.studio.zones.push(Zone {
            name: "All".into(),
            start: 0,
            end: 1000,
            color: "#ffffff".into(),
            effect: None,
        });
        assert!(studio_preview(settings, 0.0)
            .unwrap()
            .iter()
            .flatten()
            .all(|v| *v <= 26));
    }
}
