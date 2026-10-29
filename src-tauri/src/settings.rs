use serde::{Deserialize, Serialize};
use std::{io::Write, path::Path};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Settings {
    pub device_id: String,
    pub color: String,
    pub brightness: u8,
    pub count: usize,
    pub mode: String,
    pub sides: u8,
    pub reverse: bool,
    pub offset: usize,
    /// LEDs per edge in strip order: left, top, right, bottom (0 = automatic).
    pub side_left: u8,
    pub side_top: u8,
    pub side_right: u8,
    pub side_bottom: u8,
    /// Empty space at each corner, percent of the edge length (0-20).
    pub corner_gap: u8,
    pub smoothing: u8,
    pub audio_source: String,
    pub sensitivity: u8,
    pub fps: u8,
    pub screen_id: String,
    #[serde(default)]
    pub capture_source: crate::capture_source::Source,
    pub off_on_sleep: bool,
    pub off_on_display_sleep: bool,
    pub off_on_lock: bool,
    pub restore_on_start: bool,
    pub resume_capture: bool,
    pub off_on_quit: bool,
    pub effect: String,
    pub palette: String,
    #[serde(default)]
    pub custom_palette: Vec<String>,
    pub speed: u8,
    pub music_style: String,
    pub noise_floor: u8,
    pub release: u8,
    pub exclude_bars: bool,
    pub rgb_balance: [u8; 3],
    pub saturation: u16,
    pub gamma: u16,
    pub input_gain: i8,
    pub blend: u8,
    pub max_brightness: u8,
    pub transition_limit: u16,
    pub minimum_glow: u8,
    pub effect_reverse: bool,
    #[serde(default)]
    pub studio: crate::studio::Studio,
    #[serde(default)]
    pub replay: crate::sequence::Playback,
    pub mode_settings: std::collections::BTreeMap<String, ModeSettings>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            device_id: String::new(),
            color: "#9b72ff".into(),
            brightness: 38,
            count: 65,
            mode: "solid".into(),
            sides: 3,
            reverse: false,
            offset: 0,
            side_left: 0,
            side_top: 0,
            side_right: 0,
            side_bottom: 0,
            corner_gap: 0,
            smoothing: 65,
            audio_source: "system".into(),
            sensitivity: 50,
            fps: 60,
            screen_id: String::new(),
            capture_source: Default::default(),
            off_on_sleep: true,
            off_on_display_sleep: true,
            off_on_lock: true,
            restore_on_start: false,
            resume_capture: false,
            off_on_quit: true,
            effect: "rainbow".into(),
            palette: "aurora".into(),
            custom_palette: Vec::new(),
            speed: 35,
            music_style: "spectrum".into(),
            noise_floor: 0,
            release: 40,
            exclude_bars: false,
            rgb_balance: [100; 3],
            saturation: 100,
            gamma: 100,
            input_gain: 0,
            blend: 60,
            max_brightness: 100,
            transition_limit: 0,
            minimum_glow: 0,
            effect_reverse: false,
            studio: Default::default(),
            replay: Default::default(),
            mode_settings: Default::default(),
        }
    }
}
impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        if self.device_id.len() > 4096
            || !self
                .device_id
                .bytes()
                .all(|b| b.is_ascii_hexdigit() || b == b'-' || b == b's' || b == b'p')
        {
            return Err("Invalid controller binding".into());
        }
        self.capture_source.validate()?;
        self.studio.validate()?;
        self.replay.validate()?;
        if self.mode_settings.len() > 6 {
            return Err("Too many remembered modes".into());
        }
        for (mode, memory) in &self.mode_settings {
            let mut candidate = Settings {
                mode: mode.clone(),
                ..Default::default()
            };
            memory.apply(&mut candidate);
            candidate.validate()?;
        }
        if self.custom_palette.len() > 8
            || self
                .custom_palette
                .iter()
                .any(|c| !crate::sharing::valid_color(c))
            || (self.palette == "custom" && self.custom_palette.is_empty())
        {
            return Err("Use 1–8 valid colors for a custom palette".into());
        }
        if self.color.len() != 7
            || !self.color.starts_with('#')
            || !self.color.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
        {
            return Err("Invalid color".into());
        }
        if self.brightness > 100
            || !(1..=127).contains(&self.count)
            || !(3..=4).contains(&self.sides)
            || self.offset > 126
            || self.smoothing > 95
            || !(1..=100).contains(&self.sensitivity)
            || ![10, 20, 30, 45, 60].contains(&self.fps)
            || [
                self.side_left,
                self.side_top,
                self.side_right,
                self.side_bottom,
            ]
            .iter()
            .any(|n| *n > 127)
            || self.corner_gap > 20
            || self.screen_id.len() > 200
            || !(1..=100).contains(&self.speed)
            || self.noise_floor > 20
            || self.release > 100
            || self.rgb_balance.iter().any(|v| *v > 200)
            || self.saturation > 200
            || !(50..=250).contains(&self.gamma)
            || !(-24..=24).contains(&self.input_gain)
            || self.blend > 100
            || self.max_brightness > 100
            || self.transition_limit > 500
            || self.minimum_glow > 20
        {
            return Err("Settings outside supported range".into());
        }
        if !["solid", "screen", "music", "rainbow", "blend", "replay"].contains(&self.mode.as_str())
            || !["system", "microphone"].contains(&self.audio_source.as_str())
        {
            return Err("Unknown lighting mode or audio source".into());
        }
        if !["rainbow", "aurora", "breath", "comet", "ember", "custom"]
            .contains(&self.effect.as_str())
            || !["aurora", "ocean", "sunset", "mono", "custom"].contains(&self.palette.as_str())
            || !["spectrum", "bass", "vu", "wave", "beat"].contains(&self.music_style.as_str())
        {
            return Err("Unknown effect, palette or music style".into());
        }
        Ok(())
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        self.validate()?;
        save_json(self, path)
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        let s: Self = serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        s.validate()?;
        Ok(s)
    }
}

pub fn save_json(value: &impl Serialize, path: &Path) -> Result<(), String> {
    let parent = path.parent().ok_or("Invalid settings path")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let mut tmp = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    tmp.write_all(&serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    tmp.as_file().sync_all().map_err(|e| e.to_string())?;
    tmp.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_untrusted_settings() {
        for patch in [
            r#"{"count":0}"#,
            r#"{"count":128}"#,
            r#"{"brightness":101}"#,
            r#"{"offOnQuit":"false"}"#,
            r#"{"mode":"oops"}"#,
            r#"{"color":"💡fff"}"#,
            r#"{"effect":"strobe"}"#,
            r#"{"palette":"unknown"}"#,
            r#"{"musicStyle":"unknown"}"#,
            r#"{"speed":0}"#,
            r#"{"noiseFloor":21}"#,
            r#"{"release":101}"#,
            r#"{"inputGain":25}"#,
            r#"{"rgbBalance":[100,201,100]}"#,
            r#"{"gamma":0}"#,
            r#"{"maxBrightness":101}"#,
            r#"{"transitionLimit":501}"#,
            r#"{"blend":101}"#,
        ] {
            assert!(serde_json::from_str::<Settings>(patch)
                .and_then(|s| s.validate().map_err(serde::de::Error::custom))
                .is_err());
        }
    }
    #[test]
    fn atomic_replace_and_legacy_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("settings.json");
        let mut s = Settings::default();
        s.save(&p).unwrap();
        s.count = 99;
        s.save(&p).unwrap();
        assert_eq!(Settings::load(&p).unwrap(), s);
        assert_eq!(
            serde_json::from_str::<Settings>("{}").unwrap(),
            Settings::default()
        );
    }
}

/// Only scene controls are remembered. Hardware, privacy and comfort remain global.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModeSettings {
    color: String,
    brightness: u8,
    smoothing: u8,
    audio_source: String,
    sensitivity: u8,
    fps: u8,
    screen_id: String,
    #[serde(default)]
    capture_source: crate::capture_source::Source,
    effect: String,
    palette: String,
    #[serde(default)]
    custom_palette: Vec<String>,
    speed: u8,
    music_style: String,
    noise_floor: u8,
    release: u8,
    exclude_bars: bool,
    rgb_balance: [u8; 3],
    saturation: u16,
    gamma: u16,
    input_gain: i8,
    blend: u8,
    effect_reverse: bool,
    #[serde(default)]
    studio: crate::studio::Studio,
    #[serde(default)]
    replay: crate::sequence::Playback,
}
impl ModeSettings {
    pub fn capture(s: &Settings) -> Self {
        Self {
            color: s.color.clone(),
            brightness: s.brightness,
            smoothing: s.smoothing,
            audio_source: s.audio_source.clone(),
            sensitivity: s.sensitivity,
            fps: s.fps,
            screen_id: s.screen_id.clone(),
            capture_source: s.capture_source.clone(),
            effect: s.effect.clone(),
            palette: s.palette.clone(),
            custom_palette: s.custom_palette.clone(),
            speed: s.speed,
            music_style: s.music_style.clone(),
            noise_floor: s.noise_floor,
            release: s.release,
            exclude_bars: s.exclude_bars,
            rgb_balance: s.rgb_balance,
            saturation: s.saturation,
            gamma: s.gamma,
            input_gain: s.input_gain,
            blend: s.blend,
            effect_reverse: s.effect_reverse,
            studio: s.studio.clone(),
            replay: s.replay.clone(),
        }
    }
    pub fn apply(&self, s: &mut Settings) {
        s.color = self.color.clone();
        s.brightness = self.brightness;
        s.smoothing = self.smoothing;
        s.audio_source = self.audio_source.clone();
        s.sensitivity = self.sensitivity;
        s.fps = self.fps;
        s.screen_id = self.screen_id.clone();
        s.capture_source = self.capture_source.clone();
        s.effect = self.effect.clone();
        s.palette = self.palette.clone();
        s.custom_palette = self.custom_palette.clone();
        s.speed = self.speed;
        s.music_style = self.music_style.clone();
        s.noise_floor = self.noise_floor;
        s.release = self.release;
        s.exclude_bars = self.exclude_bars;
        s.rgb_balance = self.rgb_balance;
        s.saturation = self.saturation;
        s.gamma = self.gamma;
        s.input_gain = self.input_gain;
        s.blend = self.blend;
        s.effect_reverse = self.effect_reverse;
        s.studio = self.studio.clone();
        s.replay = self.replay.clone();
    }
}

/// Portable lighting choices. Device layout, display bindings, consent, lifecycle
/// and comfort limits belong to this installation, never to a shared scene.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SceneSettings {
    pub color: String,
    pub brightness: u8,
    pub mode: String,
    pub smoothing: u8,
    pub audio_source: String,
    pub sensitivity: u8,
    pub fps: u8,
    pub effect: String,
    pub palette: String,
    #[serde(default)]
    pub custom_palette: Vec<String>,
    pub speed: u8,
    pub music_style: String,
    pub noise_floor: u8,
    pub release: u8,
    pub exclude_bars: bool,
    pub rgb_balance: [u8; 3],
    pub saturation: u16,
    pub gamma: u16,
    pub input_gain: i8,
    pub blend: u8,
    pub effect_reverse: bool,
    #[serde(default)]
    pub studio: crate::studio::Studio,
    #[serde(default)]
    pub replay: crate::sequence::Playback,
}
impl SceneSettings {
    pub fn capture(s: &Settings) -> Self {
        Self {
            color: s.color.clone(),
            brightness: s.brightness,
            mode: s.mode.clone(),
            smoothing: s.smoothing,
            audio_source: s.audio_source.clone(),
            sensitivity: s.sensitivity,
            fps: s.fps,
            effect: s.effect.clone(),
            palette: s.palette.clone(),
            custom_palette: s.custom_palette.clone(),
            speed: s.speed,
            music_style: s.music_style.clone(),
            noise_floor: s.noise_floor,
            release: s.release,
            exclude_bars: s.exclude_bars,
            rgb_balance: s.rgb_balance,
            saturation: s.saturation,
            gamma: s.gamma,
            input_gain: s.input_gain,
            blend: s.blend,
            effect_reverse: s.effect_reverse,
            studio: s.studio.clone(),
            replay: s.replay.clone(),
        }
    }
    pub fn apply(&self, current: &Settings) -> Settings {
        Settings {
            color: self.color.clone(),
            brightness: self.brightness,
            mode: self.mode.clone(),
            smoothing: self.smoothing,
            audio_source: self.audio_source.clone(),
            sensitivity: self.sensitivity,
            fps: self.fps,
            effect: self.effect.clone(),
            palette: self.palette.clone(),
            custom_palette: self.custom_palette.clone(),
            speed: self.speed,
            music_style: self.music_style.clone(),
            noise_floor: self.noise_floor,
            release: self.release,
            exclude_bars: self.exclude_bars,
            rgb_balance: self.rgb_balance,
            saturation: self.saturation,
            gamma: self.gamma,
            input_gain: self.input_gain,
            blend: self.blend,
            effect_reverse: self.effect_reverse,
            studio: self.studio.clone(),
            replay: self.replay.clone(),
            ..current.clone()
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        self.apply(&Settings::default()).validate()
    }
}

/// A local profile may remember a display; it is separate from portable settings.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceBinding {
    pub screen_id: String,
    #[serde(default)]
    pub capture_source: crate::capture_source::Source,
}

/// Atomic field edits resolved against the worker's current settings. Disjoint
/// edits compose; for the same field the last processed command wins. An empty
/// patch commits current state without replaying a renderer's stale snapshot.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(transparent)]
pub struct SettingsPatch(pub serde_json::Map<String, serde_json::Value>);
impl SettingsPatch {
    pub fn apply(&self, current: &Settings) -> Result<Settings, String> {
        let mut value = serde_json::to_value(current).map_err(|e| e.to_string())?;
        let fields = value.as_object_mut().ok_or("Invalid settings")?;
        for (key, value) in &self.0 {
            if key == "modeSettings" || !fields.contains_key(key) {
                return Err(format!("Settings field cannot be edited: {key}"));
            }
            fields.insert(key.clone(), value.clone());
        }
        let mut settings: Settings = serde_json::from_value(value).map_err(|e| e.to_string())?;
        if self.0.contains_key("color") && !self.0.contains_key("studio") {
            settings.studio.bias.enabled = false;
        }
        settings.validate()?;
        Ok(settings)
    }
}
