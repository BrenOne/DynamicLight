//! Adaptive spectral-flux onset detection, not a tempo/BPM estimate.
#[derive(Default)]
pub struct Detector {
    previous: Vec<f32>,
    mean: f32,
    deviation: f32,
    elapsed: f32,
    warmup: f32,
    pub level: f32,
}
impl Detector {
    pub fn update(&mut self, bands: &[f32], dt: f32, sensitivity: u8) -> f32 {
        self.elapsed += dt;
        self.warmup += dt;
        self.level *= (-dt / 0.18).exp();
        if self.previous.len() != bands.len() {
            self.previous = vec![0.0; bands.len()];
        }
        let flux = bands
            .iter()
            .zip(&self.previous)
            .map(|(n, p)| (n - p).max(0.0))
            .sum::<f32>()
            / bands.len().max(1) as f32;
        let threshold = self.mean + self.deviation * (3.5 - sensitivity as f32 * 0.025) + 0.015;
        if self.warmup > 0.2
            && self.elapsed >= 0.22
            && flux > threshold
            && bands.iter().any(|v| *v > 0.08)
        {
            self.level = 1.0;
            self.elapsed = 0.0;
        }
        let weight = 1.0 - (-dt / 1.5).exp();
        self.deviation += ((flux - self.mean).abs() - self.deviation) * weight;
        self.mean += (flux - self.mean) * weight;
        self.previous.copy_from_slice(bands);
        self.level
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn silence_sustain_and_refractory_do_not_repeat_onsets() {
        let mut d = Detector::default();
        for _ in 0..30 {
            assert_eq!(d.update(&[0.0; 32], 1.0 / 30.0, 50), 0.0);
        }
        assert_eq!(d.update(&[0.8; 32], 1.0 / 30.0, 50), 1.0);
        for _ in 0..30 {
            assert!(d.update(&[0.8; 32], 1.0 / 30.0, 50) < 1.0);
        }
        d.update(&[0.0; 32], 1.0 / 30.0, 50);
        assert_eq!(d.update(&[0.8; 32], 1.0 / 30.0, 50), 1.0);
        d.update(&[0.0; 32], 1.0 / 30.0, 50);
        assert!(d.update(&[0.8; 32], 1.0 / 30.0, 50) < 1.0);
    }
}
