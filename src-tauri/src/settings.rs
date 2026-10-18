use serde::{Deserialize, Serialize};
use std::{io::Write, path::Path};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Settings {
    pub color: String,
    pub brightness: u8,
    pub count: usize,
    pub mode: String,
    pub sides: u8,
    pub reverse: bool,
    pub offset: usize,
    pub smoothing: u8,
    pub audio_source: String,
    pub sensitivity: u8,
    pub fps: u8,
    pub screen_id: String,
    pub off_on_sleep: bool,
    pub off_on_display_sleep: bool,
    pub off_on_lock: bool,
    pub restore_on_start: bool,
    pub resume_capture: bool,
    pub off_on_quit: bool,
    pub effect: String,
    pub palette: String,
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
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            color: "#9b72ff".into(),
            brightness: 38,
            count: 65,
            mode: "solid".into(),
            sides: 3,
            reverse: false,
            offset: 0,
            smoothing: 65,
            audio_source: "system".into(),
            sensitivity: 50,
            fps: 30,
            screen_id: String::new(),
            off_on_sleep: true,
            off_on_display_sleep: true,
            off_on_lock: true,
            restore_on_start: false,
            resume_capture: false,
            off_on_quit: true,
            effect: "rainbow".into(),
            palette: "aurora".into(),
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
        }
    }
}
impl Settings {
    pub fn validate(&self) -> Result<(), String> {
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
            || ![10, 20, 30].contains(&self.fps)
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
        if !["solid", "screen", "music", "rainbow", "blend"].contains(&self.mode.as_str())
            || !["system", "microphone"].contains(&self.audio_source.as_str())
        {
            return Err("Unknown lighting mode or audio source".into());
        }
        if !["rainbow", "aurora", "breath", "comet", "ember"].contains(&self.effect.as_str())
            || !["aurora", "ocean", "sunset", "mono"].contains(&self.palette.as_str())
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
