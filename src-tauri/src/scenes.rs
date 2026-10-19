use crate::{
    effects,
    engine::{Action, Handle, Snapshot},
    profiles::{Library, Profile},
    protocol::Rgb,
    settings::{SceneSettings, Settings, SourceBinding},
};
use tauri::{Emitter, Manager};
#[tauri::command]
pub fn starter_scenes() -> Library {
    let definitions = [
        (
            "warm-desk",
            "Warm desk",
            Settings {
                mode: "solid".into(),
                color: "#ffbf80".into(),
                brightness: 28,
                ..Default::default()
            },
        ),
        (
            "cinema",
            "Cinema",
            Settings {
                mode: "screen".into(),
                brightness: 35,
                smoothing: 80,
                exclude_bars: true,
                ..Default::default()
            },
        ),
        (
            "neon",
            "Neon",
            Settings {
                mode: "rainbow".into(),
                effect: "aurora".into(),
                palette: "aurora".into(),
                brightness: 55,
                speed: 50,
                ..Default::default()
            },
        ),
        (
            "ocean",
            "Ocean",
            Settings {
                mode: "rainbow".into(),
                effect: "aurora".into(),
                palette: "ocean".into(),
                brightness: 40,
                speed: 20,
                ..Default::default()
            },
        ),
    ];
    Library {
        profiles: definitions
            .into_iter()
            .map(|(id, name, settings)| Profile {
                id: id.into(),
                name: name.into(),
                favorite: false,
                shortcut: String::new(),
                settings: SceneSettings::capture(&settings),
                source: SourceBinding::default(),
            })
            .collect(),
        ..Default::default()
    }
}
#[tauri::command]
pub async fn apply_starter(app: tauri::AppHandle, id: String) -> Result<Snapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let preset = starter_scenes()
            .profiles
            .into_iter()
            .find(|p| p.id == id)
            .ok_or("Unknown starter scene")?;
        let state = app.state::<Handle>().request(Action::ApplyProfile {
            scene: preset.settings,
            source: preset.source,
        })?;
        let _ = app.emit("external-settings", &state);
        Ok(state)
    })
    .await
    .map_err(|e| e.to_string())?
}
/// Deliberately simulated source data; this command never starts capture or USB.
#[tauri::command]
pub fn scene_previews(settings: Vec<Settings>, seconds: f32) -> Result<Vec<Vec<Rgb>>, String> {
    if settings.len() > 8 || !seconds.is_finite() || !(0.0..=86400.0).contains(&seconds) {
        return Err("Invalid preview request".into());
    }
    settings
        .into_iter()
        .map(|mut s| {
            s.validate()?;
            s.count = 48;
            let bands: Vec<f32> = (0..32)
                .map(|i| (0.2 + 0.5 * (i as f32 * 0.35 - seconds * 2.0).sin()).max(0.0))
                .collect();
            let beat = [(-seconds.rem_euclid(0.6) / 0.18).exp(); 32];
            let mut colors = match s.mode.as_str() {
                "solid" => vec![
                    if s.studio.bias.enabled {
                        s.studio.bias.color()
                    } else {
                        crate::lighting::hex(&s.color)
                    };
                    48
                ],
                "rainbow" => effects::animated(&s, seconds),
                "replay" => vec![[0; 3]; 48],
                "music" => effects::music(
                    &s,
                    if s.music_style == "beat" {
                        &beat
                    } else {
                        &bands
                    },
                    seconds,
                ),
                _ => {
                    let screen: Vec<Rgb> = (0..48)
                        .map(|i| {
                            crate::lighting::hsv((i as f32 / 96.0 + seconds * 0.01).rem_euclid(1.0))
                        })
                        .collect();
                    if s.mode == "blend" {
                        effects::blend(&screen, &bands, s.blend)
                    } else {
                        screen
                    }
                }
            };
            s.studio
                .overlay(&mut colors, seconds, s.speed, s.effect_reverse);
            let brightness = s.brightness.min(s.max_brightness) as f32 / 100.0;
            Ok(colors
                .into_iter()
                .map(|c| c.map(|v| (v as f32 * brightness).round() as u8))
                .collect())
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn starters_are_valid_and_previews_use_effects_and_respect_limits() {
        let starters = starter_scenes();
        starters.validate().unwrap();
        assert_eq!(starters.profiles.len(), 4);
        let s = Settings {
            mode: "rainbow".into(),
            count: 48,
            brightness: 100,
            ..Default::default()
        };
        assert_eq!(
            scene_previews(vec![s.clone()], 2.0).unwrap()[0],
            effects::animated(&s, 2.0)
        );
        assert!(scene_previews(vec![s.clone(); 9], 2.0).is_err());
        assert!(scene_previews(vec![s.clone()], f32::NAN).is_err());
        assert!(scene_previews(
            vec![Settings {
                max_brightness: 0,
                ..s
            }],
            2.0
        )
        .unwrap()[0]
            .iter()
            .all(|c| *c == [0; 3]));
    }
}
