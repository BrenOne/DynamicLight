//! Bounded RGB-only recording and offline playback. No source pixels or audio.
use crate::{
    protocol::Rgb,
    settings::{save_json, Settings},
};
use serde::{Deserialize, Serialize};
use std::{
    io::Read,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use tauri::Manager;
pub const MAX_BYTES: usize = 10 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Playback {
    pub clip: String,
    pub start_ms: u32,
    pub end_ms: u32,
    pub speed: u16,
    pub looping: bool,
    pub remap: bool,
}
impl Default for Playback {
    fn default() -> Self {
        Self {
            clip: String::new(),
            start_ms: 0,
            end_ms: 0,
            speed: 100,
            looping: true,
            remap: false,
        }
    }
}
impl Playback {
    pub fn validate(&self) -> Result<(), String> {
        if self.clip.len() > 80
            || !self
                .clip
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-')
            || self.start_ms >= 60000
            || self.end_ms > 60000
            || !(25..=400).contains(&self.speed)
            || self.end_ms > 0 && self.start_ms >= self.end_ms
        {
            return Err("Replay needs a valid clip, trim range and speed from 25–400%".into());
        }
        Ok(())
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Frame {
    pub ms: u32,
    pub colors: Vec<Rgb>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Sequence {
    pub format: String,
    pub version: u8,
    pub id: String,
    pub count: usize,
    pub sides: u8,
    pub duration_ms: u32,
    pub frames: Vec<Frame>,
}
impl Sequence {
    pub fn validate(&self) -> Result<(), String> {
        if self.format != "dynamiclight-rgb"
            || self.version != 1
            || self.id.is_empty()
            || self.id.len() > 80
            || !self
                .id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-')
            || !(1..=127).contains(&self.count)
            || !(3..=4).contains(&self.sides)
            || !(1..=60000).contains(&self.duration_ms)
            || self.frames.is_empty()
            || self.frames.len() > 1800
            || self.frames[0].ms != 0
            || self.frames.windows(2).any(|w| w[0].ms >= w[1].ms)
            || self
                .frames
                .iter()
                .any(|f| f.colors.len() != self.count || f.ms >= self.duration_ms)
        {
            return Err("Invalid RGB sequence: v1, 1–127 LEDs, up to 60 seconds / 1800 ordered frames required".into());
        }
        Ok(())
    }
    fn range(&self, s: &Settings) -> Result<(u32, u32), String> {
        s.replay.validate()?;
        if s.replay.clip != self.id {
            return Err("Import the RGB sequence referenced by this scene first".into());
        }
        if (s.count != self.count || s.sides != self.sides) && !s.replay.remap {
            return Err(
                "LED layout differs. Explicitly enable proportional remapping before playback"
                    .into(),
            );
        }
        let end = if s.replay.end_ms == 0 {
            self.duration_ms
        } else {
            s.replay.end_ms
        };
        if s.replay.start_ms >= end || end > self.duration_ms {
            return Err("Trim range is outside the loaded sequence".into());
        }
        Ok((s.replay.start_ms, end))
    }
    pub fn sample(&self, s: &Settings, elapsed: Duration) -> Result<Vec<Rgb>, String> {
        let (start, end) = self.range(s)?;
        let span = u128::from(end - start);
        let time = elapsed.as_millis() * u128::from(s.replay.speed) / 100;
        let time = start
            + if s.replay.looping {
                time % span
            } else {
                time.min(span - 1)
            } as u32;
        let frame = &self.frames[self
            .frames
            .partition_point(|f| f.ms <= time)
            .saturating_sub(1)];
        Ok((0..s.count)
            .map(|i| frame.colors[((i * 2 + 1) * self.count / (s.count * 2)).min(self.count - 1)])
            .collect())
    }
    fn info(&self) -> Info {
        Info {
            id: self.id.clone(),
            count: self.count,
            sides: self.sides,
            duration_ms: self.duration_ms,
            frames: self.frames.len(),
        }
    }
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    pub id: String,
    pub count: usize,
    pub sides: u8,
    pub duration_ms: u32,
    pub frames: usize,
}
#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub recording: bool,
    pub elapsed_ms: u32,
    pub recorded_frames: usize,
    pub clip: Option<Info>,
}
struct Recorder {
    started: Instant,
    limit_ms: u32,
    interval_ms: u32,
    next_ms: u32,
    count: usize,
    sides: u8,
    frames: Vec<Frame>,
}
pub struct Deck {
    recording: Option<Recorder>,
    clip: Option<Sequence>,
    path: PathBuf,
}
impl Deck {
    pub fn load(path: PathBuf) -> Self {
        let clip = std::fs::File::open(&path)
            .ok()
            .and_then(|file| {
                let mut bytes = Vec::new();
                file.take(MAX_BYTES as u64 + 1)
                    .read_to_end(&mut bytes)
                    .ok()?;
                if bytes.len() > MAX_BYTES {
                    return None;
                }
                serde_json::from_slice::<Sequence>(&bytes).ok()
            })
            .filter(|s| s.validate().is_ok());
        Self {
            recording: None,
            clip,
            path,
        }
    }
    pub fn status(&self) -> Status {
        Status {
            recording: self.recording.is_some(),
            elapsed_ms: self.recording.as_ref().map_or(0, |r| {
                r.started.elapsed().as_millis().min(u128::from(r.limit_ms)) as u32
            }),
            recorded_frames: self.recording.as_ref().map_or(0, |r| r.frames.len()),
            clip: self.clip.as_ref().map(Sequence::info),
        }
    }
    pub fn begin(&mut self, s: &Settings, seconds: u8) -> Result<(), String> {
        if !(1..=60).contains(&seconds) || self.recording.is_some() || s.mode == "replay" {
            return Err(
                "Record 1–60 seconds from a live mode; stop an existing recording first".into(),
            );
        }
        self.recording = Some(Recorder {
            started: Instant::now(),
            limit_ms: u32::from(seconds) * 1000,
            interval_ms: 1000 / u32::from(s.fps),
            next_ms: 0,
            count: s.count,
            sides: s.sides,
            frames: vec![],
        });
        Ok(())
    }
    pub fn record(&mut self, colors: &[Rgb], s: &Settings) -> Result<(), String> {
        let Some(r) = self.recording.as_mut() else {
            return Ok(());
        };
        let ms = r.started.elapsed().as_millis().min(60000) as u32;
        if ms >= r.limit_ms || r.frames.len() >= 1800 || r.count != s.count || r.sides != s.sides {
            return self.finish();
        }
        if ms >= r.next_ms && colors.len() == r.count {
            r.frames.push(Frame {
                ms: if r.frames.is_empty() { 0 } else { ms },
                colors: colors.to_vec(),
            });
            r.next_ms = ms + r.interval_ms;
        }
        Ok(())
    }
    pub fn finish(&mut self) -> Result<(), String> {
        let r = self.recording.take().ok_or("No recording is active")?;
        let duration_ms = (r.started.elapsed().as_millis().min(u128::from(r.limit_ms)) as u32)
            .max(r.frames.last().map_or(1, |f| f.ms + 1));
        let sequence = Sequence {
            format: "dynamiclight-rgb".into(),
            version: 1,
            id: format!(
                "rgb-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ),
            count: r.count,
            sides: r.sides,
            duration_ms,
            frames: r.frames,
        };
        sequence.validate()?;
        self.clip = Some(sequence);
        self.save_current()
    }
    pub fn replace(&mut self, sequence: Sequence) -> Result<(), String> {
        if self.recording.is_some() {
            return Err("Stop recording before importing a sequence".into());
        }
        sequence.validate()?;
        save_json(&sequence, &self.path)?;
        self.clip = Some(sequence);
        Ok(())
    }
    fn save_current(&self) -> Result<(), String> {
        save_json(
            self.clip.as_ref().ok_or("No RGB sequence is loaded")?,
            &self.path,
        )
    }
    pub fn export(&self, path: &Path) -> Result<(), String> {
        save_json(self.clip.as_ref().ok_or("No RGB sequence is loaded")?, path)
    }
    pub fn validate_playback(&self, s: &Settings) -> Result<(), String> {
        if self.recording.is_some() {
            return Err("Finish recording before playback".into());
        }
        self.clip
            .as_ref()
            .ok_or("Import or record an RGB sequence first")?
            .range(s)
            .map(|_| ())
    }
    pub fn sample(&self, s: &Settings, elapsed: Duration) -> Result<Vec<Rgb>, String> {
        self.clip
            .as_ref()
            .ok_or("Import or record an RGB sequence first")?
            .sample(s, elapsed)
    }
}
#[tauri::command]
pub async fn import_sequence(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    text: String,
) -> Result<crate::engine::Snapshot, String> {
    if window.label() != "main" || text.len() > MAX_BYTES {
        return Err("Import a sequence of at most 10 MiB in the studio".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let sequence: Sequence = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        sequence.validate()?;
        app.state::<crate::engine::Handle>()
            .request(crate::engine::Action::LoadSequence { sequence })
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn export_sequence(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
) -> Result<String, String> {
    if window.label() != "main" {
        return Err("Export sequences in the studio".into());
    }
    let filename = format!(
        "DynamicLight-rgb-{}.json",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    );
    let path = app
        .path()
        .download_dir()
        .map_err(|e| e.to_string())?
        .join(filename);
    let output = path.to_string_lossy().into_owned();
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<crate::engine::Handle>()
            .request(crate::engine::Action::ExportSequence { path })
    })
    .await
    .map_err(|e| e.to_string())??;
    Ok(output)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn clip() -> Sequence {
        Sequence {
            format: "dynamiclight-rgb".into(),
            version: 1,
            id: "test".into(),
            count: 2,
            sides: 3,
            duration_ms: 1000,
            frames: vec![
                Frame {
                    ms: 0,
                    colors: vec![[255, 0, 0], [0, 0, 255]],
                },
                Frame {
                    ms: 500,
                    colors: vec![[0, 255, 0]; 2],
                },
            ],
        }
    }
    #[test]
    fn replay_trim_speed_loop_and_explicit_layout_remapping() {
        let sequence = clip();
        sequence.validate().unwrap();
        let mut s = Settings {
            count: 2,
            replay: Playback {
                clip: "test".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(
            sequence.sample(&s, Duration::from_millis(600)).unwrap(),
            vec![[0, 255, 0]; 2]
        );
        assert_eq!(
            sequence.sample(&s, Duration::from_millis(1100)).unwrap()[0],
            [255, 0, 0]
        );
        s.replay.looping = false;
        assert_eq!(
            sequence.sample(&s, Duration::from_secs(2)).unwrap()[0],
            [0, 255, 0]
        );
        s.replay.start_ms = 500;
        s.replay.end_ms = 900;
        s.replay.speed = 200;
        assert_eq!(sequence.sample(&s, Duration::ZERO).unwrap()[0], [0, 255, 0]);
        s.count = 4;
        assert!(sequence.sample(&s, Duration::ZERO).is_err());
        s.replay.remap = true;
        s.replay.start_ms = 0;
        assert_eq!(
            sequence.sample(&s, Duration::ZERO).unwrap(),
            vec![[255, 0, 0], [255, 0, 0], [0, 0, 255], [0, 0, 255]]
        );
        s.replay.clip = "different".into();
        assert!(sequence.sample(&s, Duration::ZERO).is_err());
    }
    #[test]
    fn malformed_and_oversized_sequences_are_rejected() {
        let mut s = clip();
        s.frames[1].ms = 0;
        assert!(s.validate().is_err());
        s = clip();
        s.frames[1].colors.pop();
        assert!(s.validate().is_err());
        s = clip();
        s.duration_ms = 60001;
        assert!(s.validate().is_err());
    }
    #[test]
    fn recording_is_bounded_and_persists_only_rgb_frames() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sequence.json");
        let settings = Settings {
            count: 2,
            ..Default::default()
        };
        let mut deck = Deck::load(path.clone());
        assert!(deck.begin(&settings, 61).is_err());
        deck.begin(&settings, 1).unwrap();
        assert!(deck.begin(&settings, 1).is_err());
        deck.record(&[[1, 2, 3], [4, 5, 6]], &settings).unwrap();
        deck.recording.as_mut().unwrap().started -= Duration::from_secs(2);
        deck.record(&[[7, 8, 9]; 2], &settings).unwrap();
        assert!(!deck.status().recording);
        let stored: Sequence =
            serde_json::from_slice(&std::fs::read(path.clone()).unwrap()).unwrap();
        assert_eq!(stored.frames.len(), 1);
        assert_eq!(stored.duration_ms, 1000);
        assert_eq!(stored.frames[0].colors, [[1, 2, 3], [4, 5, 6]]);
        assert_eq!(Deck::load(path).status().clip.unwrap().id, stored.id);
        let mut value = serde_json::to_value(stored).unwrap();
        value["screenshot"] = "private".into();
        assert!(serde_json::from_value::<Sequence>(value).is_err());
    }
}
