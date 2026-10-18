//! Pure, bounded lighting patterns. Time is supplied by the native worker.
use crate::{lighting, protocol::Rgb, settings::Settings};
use std::f32::consts::TAU;

fn scale(rgb: Rgb, level: f32) -> Rgb {
    rgb.map(|c| (c as f32 * level.clamp(0.0, 1.0)).round() as u8)
}
pub fn blend(colors: &[Rgb], bands: &[f32], strength: u8) -> Vec<Rgb> {
    let peak = bands.iter().copied().fold(0.0, f32::max);
    // Silence keeps screen brightness; audio accents dim between peaks without amplification.
    let activity = (peak * 12.0).clamp(0.0, 1.0);
    let level = 1.0 - strength as f32 / 100.0 * activity * (1.0 - peak);
    colors.iter().map(|c| scale(*c, level)).collect()
}
pub fn palette(s: &Settings, position: f32) -> Rgb {
    let stops = match s.palette.as_str() {
        "ocean" => [[20, 60, 255], [0, 220, 220], [80, 160, 255]],
        "sunset" => [[255, 55, 90], [255, 155, 45], [140, 45, 230]],
        "mono" => return lighting::hex(&s.color),
        _ => [[80, 245, 180], [70, 110, 255], [210, 65, 245]],
    };
    let p = position.rem_euclid(1.0) * 3.0;
    // Floating-point rem_euclid can round a tiny negative phase up to 1.0.
    let i = p.floor() as usize % 3;
    std::array::from_fn(|c| {
        (stops[i][c] as f32 * (1.0 - p.fract()) + stops[(i + 1) % 3][c] as f32 * p.fract()).round()
            as u8
    })
}
pub fn animated(s: &Settings, seconds: f32) -> Vec<Rgb> {
    let t = seconds * (0.03 + s.speed as f32 / 250.0);
    (0..s.count)
        .map(|i| {
            let x = i as f32 / s.count as f32;
            match s.effect.as_str() {
                "breath" => scale(palette(s, x), 0.08 + 0.92 * (0.5 - 0.5 * (t * TAU).cos())),
                "comet" => {
                    let distance = (t - x).rem_euclid(1.0);
                    scale(
                        palette(s, x + t * 0.1),
                        (1.0 - distance / 0.3).max(0.0).powi(2),
                    )
                }
                "ember" => {
                    let glow = 0.55
                        + 0.22 * (x * 19.0 + t * 7.0).sin()
                        + 0.15 * (x * 37.0 - t * 11.0).sin();
                    scale([255, (45.0 + glow * 75.0) as u8, 12], glow)
                }
                "aurora" => scale(
                    palette(s, x * 0.8 + t * 0.25),
                    0.6 + 0.4 * (x * TAU * 2.0 - t * TAU).sin(),
                ),
                _ => lighting::hsv((x + t * 0.25).rem_euclid(1.0)),
            }
        })
        .collect()
}
pub fn music(s: &Settings, levels: &[f32], seconds: f32) -> Vec<Rgb> {
    if levels.is_empty() {
        return vec![[0; 3]; s.count];
    }
    let peak = levels.iter().copied().fold(0.0, f32::max);
    let bass = levels.iter().take(7).copied().fold(0.0, f32::max);
    (0..s.count)
        .map(|i| {
            let x = i as f32 / s.count as f32;
            let band = levels[(i * levels.len() / s.count).min(levels.len() - 1)];
            let (position, level) = match s.music_style.as_str() {
                "bass" => (x, bass),
                "vu" => (
                    x * 0.65,
                    ((peak - (2.0 * x - 1.0).abs()) * 12.0).clamp(0.0, 1.0),
                ),
                "wave" => (
                    x - seconds * (0.01 + s.speed as f32 / 1000.0),
                    peak * (0.55 + 0.45 * (x * TAU * 2.0 - seconds * 2.0).sin()),
                ),
                _ => (x, band),
            };
            scale(palette(s, position), level)
        })
        .collect()
}
/// Fast attack, time-based release, independent of configured frame rate.
pub fn envelope(previous: &mut Vec<f32>, incoming: &[f32], dt: f32, release: u8) {
    if previous.len() != incoming.len() {
        previous.resize(incoming.len(), 0.0);
    }
    let decay = (-dt / (0.04 + release as f32 * 0.008)).exp();
    for (old, value) in previous.iter_mut().zip(incoming) {
        *old = value.max(*old * decay).clamp(0.0, 1.0);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn blend_silence_is_screen_and_never_amplifies_colors() {
        let colors = [[100, 200, 255]];
        assert_eq!(blend(&colors, &[0.0; 32], 100), colors);
        assert_eq!(blend(&colors, &[0.5; 32], 0), colors);
        assert_eq!(blend(&colors, &[0.5; 32], 100), [[50, 100, 128]]);
    }
    #[test]
    fn palettes_wrap_negative_and_boundary_phases() {
        for name in ["aurora", "ocean", "sunset", "mono"] {
            let s = Settings {
                palette: name.into(),
                ..Settings::default()
            };
            assert_eq!(palette(&s, 0.0), palette(&s, 1.0));
            assert_eq!(palette(&s, 0.25), palette(&s, -0.75));
            assert_eq!(palette(&s, 0.0), palette(&s, -f32::EPSILON / 8.0));
        }
    }
    #[test]
    fn effects_move_and_support_every_strip_size() {
        for effect in ["rainbow", "aurora", "breath", "comet", "ember"] {
            for count in [1, 65, 127] {
                let s = Settings {
                    effect: effect.into(),
                    count,
                    ..Settings::default()
                };
                assert_eq!(animated(&s, 0.0).len(), count);
                assert_ne!(animated(&s, 0.0), animated(&s, 2.3), "{effect}");
            }
        }
    }
    #[test]
    fn all_music_styles_are_dark_on_silence_and_react_to_signal() {
        for style in ["spectrum", "bass", "vu", "wave"] {
            let s = Settings {
                music_style: style.into(),
                ..Settings::default()
            };
            assert!(music(&s, &[0.0; 32], 1.0).iter().all(|c| *c == [0; 3]));
            assert!(music(&s, &[0.8; 32], 1.0).iter().any(|c| *c != [0; 3]));
        }
    }
    #[test]
    fn release_is_frame_rate_independent_and_reaches_silence() {
        let mut a = vec![1.0];
        let mut b = a.clone();
        for _ in 0..30 {
            envelope(&mut a, &[0.0], 1.0 / 30.0, 40);
        }
        for _ in 0..10 {
            envelope(&mut b, &[0.0], 0.1, 40);
        }
        assert!((a[0] - b[0]).abs() < 0.0001);
        for _ in 0..200 {
            envelope(&mut a, &[0.0], 0.1, 40);
        }
        assert!(a[0] < 0.00001);
    }
}
