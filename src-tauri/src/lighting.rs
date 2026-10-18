use crate::{protocol::Rgb, settings::Settings};
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
pub fn sample(data: &[u8], width: u32, height: u32, settings: &Settings) -> Vec<Rgb> {
    sample_strided(data, width, height, width as usize * 4, false, settings)
}
pub fn sample_strided(
    data: &[u8],
    width: u32,
    height: u32,
    stride: usize,
    bgra: bool,
    settings: &Settings,
) -> Vec<Rgb> {
    if width == 0
        || height == 0
        || stride < width as usize * 4
        || data.len() < stride * height as usize
    {
        return vec![[0; 3]; settings.count];
    }
    positions(settings.count, settings.sides, width as f32 / height as f32)
        .iter()
        .map(|&(x, y)| {
            let cx = (x * (width - 1) as f32).round() as u32;
            let cy = (y * (height - 1) as f32).round() as u32;
            let rx = (width as f32 * 0.035).round().max(1.0) as u32;
            let ry = (height as f32 * 0.06).round().max(1.0) as u32;
            let mut sum = [0u32; 3];
            let mut n = 0;
            // Bounded sampling cost, independent of desktop resolution.
            for py in (cy.saturating_sub(ry)..=(cy + ry).min(height - 1))
                .step_by((ry / 4).max(1) as usize)
            {
                for px in (cx.saturating_sub(rx)..=(cx + rx).min(width - 1))
                    .step_by((rx / 4).max(1) as usize)
                {
                    let p = py as usize * stride + px as usize * 4;
                    for c in 0..3 {
                        sum[c] += data[p + if bgra { 2 - c } else { c }] as u32;
                    }
                    n += 1;
                }
            }
            sum.map(|v| (v as f32 / n as f32).round() as u8)
        })
        .collect()
}
pub fn transform(colors: &[Rgb], previous: &[[f32; 3]], s: &Settings) -> Vec<[f32; 3]> {
    colors
        .iter()
        .enumerate()
        .map(|(i, rgb)| {
            std::array::from_fn(|c| {
                let target = rgb[c] as f32 * s.brightness as f32 / 100.0;
                if s.mode != "solid" && previous.len() == colors.len() {
                    previous[i][c] * s.smoothing as f32 / 100.0
                        + target * (1.0 - s.smoothing as f32 / 100.0)
                } else {
                    target
                }
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
            prev = transform(&[[255; 3]], &prev, &s);
        }
        assert!(prev[0][0] < 0.01);
        assert_eq!(
            reorder(&[[1; 3], [2; 3], [3; 3]], true, 1),
            vec![[2; 3], [1; 3], [3; 3]]
        );
    }
}
