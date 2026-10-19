use crate::{protocol::Rgb, settings::Settings};
#[derive(Default)]
pub struct Crop {
    active: (u32, u32),
    candidate: (u32, u32),
    since: Option<std::time::Instant>,
    size: (u32, u32),
}
impl Crop {
    fn update(
        &mut self,
        data: &[u8],
        width: u32,
        height: u32,
        stride: usize,
        enabled: bool,
    ) -> (u32, u32) {
        if !enabled || self.size != (width, height) {
            *self = Self {
                size: (width, height),
                ..Default::default()
            };
        }
        if !enabled {
            return (0, 0);
        }
        let dark = |x: u32, y: u32| {
            let p = y as usize * stride + x as usize * 4;
            data[p..p + 3].iter().all(|v| *v <= 6)
        };
        // Symmetric borders only, sampled at a bounded 32 × 32 grid per axis.
        let row_dark = |y| (0..32).all(|i| dark(i * (width - 1) / 31, y));
        let col_dark = |x| (0..32).all(|i| dark(x, i * (height - 1) / 31));
        let mut top = 0;
        let mut left = 0;
        for step in 1..=32 {
            let y = step * height / 128;
            if y == 0 || !row_dark(y - 1) || !row_dark(height - y) {
                break;
            }
            top = y;
        }
        for step in 1..=32 {
            let x = step * width / 128;
            if x == 0 || !col_dark(x - 1) || !col_dark(width - x) {
                break;
            }
            left = x;
        }
        // A black frame is not evidence of a new aspect ratio. Keep the last crop.
        if row_dark(height / 2) && col_dark(width / 2) {
            return self.active;
        }
        let candidate = (left, top);
        if candidate != self.candidate {
            self.candidate = candidate;
            self.since = Some(std::time::Instant::now());
        }
        if self
            .since
            .is_some_and(|t| t.elapsed() >= std::time::Duration::from_millis(600))
        {
            self.active = candidate;
        }
        self.active
    }
}
fn balance(rgb: [f32; 3], s: &Settings) -> Rgb {
    let luma = rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722;
    std::array::from_fn(|c| {
        let value =
            ((luma + (rgb[c] - luma) * s.saturation as f32 / 100.0) / 255.0).clamp(0.0, 1.0);
        (255.0 * value.powf(s.gamma as f32 / 100.0) * s.rgb_balance[c] as f32 / 100.0)
            .round()
            .clamp(0.0, 255.0) as u8
    })
}
pub fn hex(color: &str) -> Rgb {
    [1, 3, 5].map(|i| u8::from_str_radix(&color[i..i + 2], 16).unwrap_or(0))
}
pub fn hsv(h: f32) -> Rgb {
    [0.0, 2.0 / 3.0, 1.0 / 3.0].map(|shift| {
        (255.0 * (0.2 + 0.8 * ((((h + shift) % 1.0) * 6.0 - 3.0).abs() - 1.0).clamp(0.0, 1.0)))
            .round() as u8
    })
}
pub fn positions(count: usize, sides: u8, aspect: f32) -> Vec<(f32, f32)> {
    let perimeter = 2.0 + aspect * if sides == 4 { 2.0 } else { 1.0 };
    (0..count)
        .map(|i| {
            let p = (i as f32 + 0.5) / count as f32 * perimeter;
            if p < 1.0 {
                (0.0, 1.0 - p)
            } else if p < 1.0 + aspect {
                ((p - 1.0) / aspect, 0.0)
            } else if p < 2.0 + aspect {
                (1.0, p - 1.0 - aspect)
            } else {
                (1.0 - (p - 2.0 - aspect) / aspect, 1.0)
            }
        })
        .collect()
}
#[cfg(test)]
pub fn sample(data: &[u8], width: u32, height: u32, settings: &Settings) -> Vec<Rgb> {
    sample_strided(data, width, height, width as usize * 4, false, settings)
}
#[cfg(test)]
pub fn sample_strided(
    data: &[u8],
    width: u32,
    height: u32,
    stride: usize,
    bgra: bool,
    settings: &Settings,
) -> Vec<Rgb> {
    sample_tracked(
        data,
        width,
        height,
        stride,
        bgra,
        settings,
        &mut Crop::default(),
    )
}
#[cfg(any(test, target_os = "windows"))]
pub fn sample_tracked(
    data: &[u8],
    width: u32,
    height: u32,
    stride: usize,
    bgra: bool,
    settings: &Settings,
    crop: &mut Crop,
) -> Vec<Rgb> {
    sample_geometry(
        data,
        (width, height, stride, bgra),
        settings,
        crop,
        width as f32 / height.max(1) as f32,
    )
}
pub fn sample_geometry(
    data: &[u8],
    frame: (u32, u32, usize, bool),
    settings: &Settings,
    crop: &mut Crop,
    aspect: f32,
) -> Vec<Rgb> {
    let (width, height, stride, bgra) = frame;
    if width == 0
        || height == 0
        || stride < width as usize * 4
        || data.len() < stride * height as usize
    {
        return vec![[0; 3]; settings.count];
    }
    let (left, top) = crop.update(data, width, height, stride, settings.exclude_bars);
    let (cw, ch) = (width - 2 * left, height - 2 * top);
    // LED edge allocation follows the physical display. Cropping changes where
    // each LED samples the picture, not which physical LEDs belong to an edge.
    positions(settings.count, settings.sides, aspect)
        .iter()
        .map(|&(x, y)| {
            let cx = left + (x * (cw - 1) as f32).round() as u32;
            let cy = top + (y * (ch - 1) as f32).round() as u32;
            let rx = (width as f32 * 0.035).round().max(1.0) as u32;
            let ry = (height as f32 * 0.06).round().max(1.0) as u32;
            let mut sum = [0u32; 3];
            let mut n = 0;
            // Bounded sampling cost, independent of desktop resolution.
            for py in (cy.saturating_sub(ry).max(top)..=(cy + ry).min(height - 1 - top))
                .step_by((ry / 4).max(1) as usize)
            {
                for px in (cx.saturating_sub(rx).max(left)..=(cx + rx).min(width - 1 - left))
                    .step_by((rx / 4).max(1) as usize)
                {
                    let p = py as usize * stride + px as usize * 4;
                    for c in 0..3 {
                        sum[c] += data[p + if bgra { 2 - c } else { c }] as u32;
                    }
                    n += 1;
                }
            }
            balance(sum.map(|v| v as f32 / n as f32), settings)
        })
        .collect()
}
pub fn transform(colors: &[Rgb], previous: &[[f32; 3]], s: &Settings, dt: f32) -> Vec<[f32; 3]> {
    colors
        .iter()
        .enumerate()
        .map(|(i, rgb)| {
            std::array::from_fn(|c| {
                let target = (rgb[c] as f32).max(s.minimum_glow as f32 * 2.55)
                    * s.brightness.min(s.max_brightness) as f32
                    / 100.0;
                let target = if s.transition_limit > 0 {
                    let step = s.transition_limit as f32 * 2.55 * dt;
                    let old = if previous.len() == colors.len() {
                        previous[i][c]
                    } else {
                        0.0
                    };
                    target.clamp(old - step, old + step)
                } else {
                    target
                };
                let value = if s.mode != "solid" && previous.len() == colors.len() {
                    let retention = (s.smoothing as f32 / 100.0).powf(dt * 30.0);
                    previous[i][c] * retention + target * (1.0 - retention)
                } else {
                    target
                };
                value.min(s.max_brightness as f32 * 2.55)
            })
        })
        .collect()
}
pub fn reorder(colors: &[Rgb], reverse: bool, offset: usize) -> Vec<Rgb> {
    (0..colors.len())
        .map(|i| {
            let j = (i + offset) % colors.len();
            colors[if reverse { colors.len() - 1 - j } else { j }]
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cropping_keeps_the_physical_edge_allocation() {
        let (w, h) = (256usize, 128usize);
        let mut pixels = vec![0; w * h * 4];
        for y in 16..112 {
            for x in 0..w {
                let rgb = if y < 25 {
                    [0, 220, 0]
                } else if x < 16 {
                    [220, 0, 0]
                } else if x > w - 17 {
                    [0, 0, 220]
                } else {
                    [80, 80, 80]
                };
                pixels[(y * w + x) * 4..(y * w + x) * 4 + 3].copy_from_slice(&rgb);
            }
        }
        let mut crop = Crop {
            active: (0, 16),
            candidate: (0, 16),
            size: (w as u32, h as u32),
            since: Some(std::time::Instant::now() - std::time::Duration::from_secs(1)),
        };
        let s = Settings {
            count: 16,
            exclude_bars: true,
            ..Default::default()
        };
        let colors = sample_tracked(&pixels, w as u32, h as u32, w * 4, false, &s, &mut crop);
        // A 2:1 monitor has four LEDs on each vertical edge. Reallocating for
        // cropped content would incorrectly turn the fourth left LED green.
        assert!(colors[3][0] > colors[3][1]);
        assert!(colors[4][1] > colors[4][0]);
        assert!(colors[12][2] > colors[12][1]);
    }
    #[test]
    fn letterbox_requires_stability_and_black_frames_keep_crop() {
        let (w, h) = (256, 128);
        let mut pixels = vec![0; w * h * 4];
        for y in 16..112 {
            for x in 0..w {
                pixels[(y * w + x) * 4] = 200;
            }
        }
        let mut crop = Crop::default();
        assert_eq!(
            crop.update(&pixels, w as u32, h as u32, w * 4, true),
            (0, 0)
        );
        crop.since = Some(std::time::Instant::now() - std::time::Duration::from_secs(1));
        assert_eq!(
            crop.update(&pixels, w as u32, h as u32, w * 4, true),
            (0, 16)
        );
        let s = Settings {
            exclude_bars: true,
            ..Default::default()
        };
        let colors = sample_tracked(&pixels, w as u32, h as u32, w * 4, false, &s, &mut crop);
        assert!(colors.iter().all(|c| *c == [200, 0, 0]));
        pixels.fill(0);
        assert_eq!(
            crop.update(&pixels, w as u32, h as u32, w * 4, true),
            (0, 16)
        );
        assert_eq!(
            crop.update(&pixels, w as u32, h as u32, w * 4, false),
            (0, 0)
        );
    }
    #[test]
    fn color_adjustments_and_hard_cap_are_bounded() {
        let s = Settings {
            saturation: 0,
            rgb_balance: [100, 50, 0],
            ..Default::default()
        };
        let rgb = balance([200.0, 100.0, 0.0], &s);
        assert!(rgb[0] > rgb[1]);
        assert_eq!(rgb[2], 0);
        let s = Settings {
            brightness: 100,
            max_brightness: 20,
            transition_limit: 10,
            smoothing: 90,
            mode: "screen".into(),
            ..Default::default()
        };
        assert!(transform(&[[255; 3]], &[[255.0; 3]], &s, 0.1)[0]
            .iter()
            .all(|v| *v <= 51.0));
        assert!(transform(&[[255; 3]], &[], &s, 0.1)[0][0] < 3.0);
    }
    #[test]
    fn bgra_row_padding_is_not_sampled() {
        let mut pixels = vec![170; 40];
        for y in 0..2 {
            for x in 0..4 {
                pixels[y * 20 + x * 4..y * 20 + x * 4 + 4].copy_from_slice(&[3, 2, 1, 255]);
            }
        }
        let colors = sample_strided(&pixels, 4, 2, 20, true, &Settings::default());
        assert!(colors.iter().all(|rgb| *rgb == [1, 2, 3]));
    }
    #[test]
    fn edge_sampling_is_spatial() {
        let mut data = vec![0; 160 * 90 * 4];
        for y in 0..90 {
            for x in 0..160 {
                data[(y * 160 + x) * 4 + if x < 80 { 0 } else { 2 }] = 255;
            }
        }
        let colors = sample(&data, 160, 90, &Settings::default());
        assert_eq!(colors[0], [255, 0, 0]);
        assert_eq!(colors[64], [0, 0, 255]);
    }
    #[test]
    fn smoothing_reaches_black_without_rounding_residue() {
        let s = Settings {
            mode: "screen".into(),
            brightness: 0,
            ..Settings::default()
        };
        let mut prev = vec![[100.0; 3]];
        for _ in 0..80 {
            prev = transform(&[[255; 3]], &prev, &s, 1.0 / 30.0);
        }
        assert!(prev[0][0] < 0.01);
        assert_eq!(
            reorder(&[[1; 3], [2; 3], [3; 3]], true, 1),
            vec![[2; 3], [1; 3], [3; 3]]
        );
    }
}
