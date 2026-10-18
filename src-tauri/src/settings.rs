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
    pub off_on_quit: bool,
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
            off_on_quit: true,
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
        {
            return Err("Settings outside supported range".into());
        }
        if !["solid", "screen", "music", "rainbow"].contains(&self.mode.as_str())
            || !["system", "microphone"].contains(&self.audio_source.as_str())
        {
            return Err("Unknown lighting mode or audio source".into());
        }
        Ok(())
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        self.validate()?;
        let parent = path.parent().ok_or("Invalid settings path")?;
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let mut tmp = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
        tmp.write_all(&serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        tmp.as_file().sync_all().map_err(|e| e.to_string())?;
        tmp.persist(path).map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        let s: Self = serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        s.validate()?;
        Ok(s)
    }
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
