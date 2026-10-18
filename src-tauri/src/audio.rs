use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rustfft::{num_complex::Complex, Fft, FftPlanner};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
const SIZE: usize = 1024;
pub struct Samples {
    data: VecDeque<f32>,
    updated: Instant,
    pub error: Option<String>,
    pub received: u64,
}
impl Samples {
    pub(crate) fn new() -> Self {
        Self {
            data: VecDeque::with_capacity(SIZE),
            updated: Instant::now(),
            error: None,
            received: 0,
        }
    }
    pub fn push(&mut self, values: impl Iterator<Item = f32>) {
        for v in values {
            if self.data.len() == SIZE {
                self.data.pop_front();
            }
            self.data.push_back(if v.is_finite() { v } else { 0.0 });
            self.received += 1;
        }
        self.updated = Instant::now();
    }
}
pub type Feed = Arc<Mutex<Samples>>;
pub struct Audio {
    _stream: Source,
    feed: Feed,
    fft: Arc<dyn Fft<f32>>,
    rate: f32,
}
pub struct Analysis {
    pub bands: Vec<f32>,
    pub rms: f32,
    pub received: u64,
}
enum Source {
    Cpal(#[allow(dead_code)] cpal::Stream),
    #[cfg(target_os = "macos")]
    Mac(#[allow(dead_code)] super::platform::MacAudio),
    #[cfg(target_os = "linux")]
    Linux(#[allow(dead_code)] super::linux::SystemAudio),
}
impl Audio {
    pub fn start(system: bool) -> Result<Self, String> {
        let feed = Arc::new(Mutex::new(Samples::new()));
        #[cfg(target_os = "linux")]
        if system {
            return Ok(Self {
                _stream: Source::Linux(super::linux::SystemAudio::start(feed.clone())?),
                feed,
                fft: FftPlanner::new().plan_fft_forward(SIZE),
                rate: 48000.0,
            });
        }
        #[cfg(target_os = "macos")]
        if system {
            let source = super::platform::MacAudio::start(feed.clone())?;
            return Ok(Self {
                _stream: Source::Mac(source),
                feed,
                fft: FftPlanner::new().plan_fft_forward(SIZE),
                rate: 48000.0,
            });
        }
        #[cfg(target_os = "macos")]
        super::platform::microphone_permission()?;
        let host = cpal::default_host();
        let device = if system {
            host.default_output_device()
        } else {
            host.default_input_device()
        }
        .ok_or("No audio device available")?;
        let config = if system {
            device.default_output_config()
        } else {
            device.default_input_config()
        }
        .map_err(|e| e.to_string())?;
        let channels = config.channels() as usize;
        let rate = config.sample_rate().0 as f32;
        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => {
                build::<f32>(&device, &config.into(), channels, feed.clone())
            }
            cpal::SampleFormat::I16 => {
                build::<i16>(&device, &config.into(), channels, feed.clone())
            }
            cpal::SampleFormat::I32 => {
                build::<i32>(&device, &config.into(), channels, feed.clone())
            }
            cpal::SampleFormat::U16 => {
                build::<u16>(&device, &config.into(), channels, feed.clone())
            }
            format => return Err(format!("Unsupported audio format: {format}")),
        }?;
        stream.play().map_err(|e| e.to_string())?;
        Ok(Self {
            _stream: Source::Cpal(stream),
            feed,
            fft: FftPlanner::new().plan_fft_forward(SIZE),
            rate,
        })
    }
    pub fn analyze(&self, sensitivity: u8, noise_floor: u8) -> Result<Analysis, String> {
        let samples = self.feed.lock().map_err(|e| e.to_string())?;
        if let Some(ref e) = samples.error {
            return Err(e.clone());
        }
        let received = samples.received;
        if samples.updated.elapsed() > Duration::from_millis(250) {
            return Ok(Analysis {
                bands: vec![0.0; 32],
                rms: 0.0,
                received,
            });
        }
        let rms = (samples.data.iter().map(|v| v * v).sum::<f32>()
            / samples.data.len().max(1) as f32)
            .sqrt()
            .clamp(0.0, 1.0);
        let mut data = vec![Complex::new(0.0, 0.0); SIZE];
        for (i, v) in samples.data.iter().enumerate() {
            data[i].re =
                *v * (0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / (SIZE - 1) as f32).cos());
        }
        drop(samples);
        self.fft.process(&mut data);
        Ok(Analysis {
            bands: if rms <= noise_floor as f32 / 100.0 {
                vec![0.0; 32]
            } else {
                bands(&data, self.rate, 32, sensitivity)
            },
            rms,
            received,
        })
    }
}
fn bands(data: &[Complex<f32>], rate: f32, count: usize, sensitivity: u8) -> Vec<f32> {
    (0..count)
        .map(|i| {
            let lo = (40.0 * 400.0f32.powf(i as f32 / count as f32) * SIZE as f32 / rate) as usize;
            let hi =
                (40.0 * 400.0f32.powf((i + 1) as f32 / count as f32) * SIZE as f32 / rate) as usize;
            let amplitude = data
                [lo.clamp(1, SIZE / 2 - 1)..=hi.clamp(lo.clamp(1, SIZE / 2 - 1), SIZE / 2 - 1)]
                .iter()
                .map(|v| v.norm() * 4.0 / SIZE as f32)
                .fold(0.0, f32::max);
            // Soft compression preserves quiet passages without lighting on digital silence.
            (amplitude.sqrt() * (0.5 + sensitivity as f32 / 25.0)).clamp(0.0, 1.0)
        })
        .collect()
}
fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    channels: usize,
    feed: Feed,
) -> Result<cpal::Stream, String>
where
    T: cpal::SizedSample,
    f32: cpal::FromSample<T>,
{
    let errors = feed.clone();
    device
        .build_input_stream(
            config,
            move |data: &[T], _| {
                if let Ok(mut s) = feed.try_lock() {
                    s.push(data.chunks_exact(channels).map(|frame| {
                        frame
                            .iter()
                            .map(|v| <f32 as cpal::FromSample<T>>::from_sample_(*v))
                            .sum::<f32>()
                            / channels as f32
                    }));
                }
            },
            move |e| {
                if let Ok(mut s) = errors.lock() {
                    s.error = Some(format!("Audio capture ended: {e}. Start capture to retry."));
                }
            },
            None,
        )
        .map_err(|e| e.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_audio_buffer_and_silence() {
        let mut s = Samples::new();
        s.push((0..10000).map(|_| 0.0));
        assert_eq!(s.data.len(), SIZE);
        assert!(bands(&vec![Complex::new(0.0, 0.0); SIZE], 48000.0, 65, 50)
            .iter()
            .all(|v| *v == 0.0));
    }
    #[test]
    fn sine_has_frequency_localization() {
        let mut data: Vec<_> = (0..SIZE)
            .map(|i| {
                Complex::new(
                    (std::f32::consts::TAU * 32.0 * i as f32 / SIZE as f32).sin(),
                    0.0,
                )
            })
            .collect();
        FftPlanner::new().plan_fft_forward(SIZE).process(&mut data);
        let values = bands(&data, 48000.0, 65, 50);
        let max = values.iter().copied().fold(0.0, f32::max);
        assert!(max > 0.9);
        assert!(values.iter().filter(|v| **v > 0.1).count() < 5);
    }
}
