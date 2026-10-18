use crate::{lighting, protocol::Rgb, settings::Settings};
use serde::Serialize;
use xcap::Monitor;
#[derive(Serialize)]
pub struct Screen {
    pub id: String,
    pub name: String,
}
pub fn screens() -> Result<Vec<Screen>, String> {
    check_session()?;
    Monitor::all()
        .map_err(|e| e.to_string())?
        .iter()
        .map(|m| {
            Ok(Screen {
                id: m.id().map_err(|e| e.to_string())?.to_string(),
                name: format!(
                    "{} · {} × {}",
                    m.friendly_name().unwrap_or_else(|_| "Display".into()),
                    m.width().unwrap_or(0),
                    m.height().unwrap_or(0)
                ),
            })
        })
        .collect()
}
/// Re-enumerate instead of trusting cached monitor handles after docking/resolution changes.
pub fn source_signature(id: &str) -> Result<String, String> {
    check_session()?;
    let all = Monitor::all().map_err(|e| e.to_string())?;
    let monitor = if id.is_empty() {
        all.iter()
            .find(|m| m.is_primary().unwrap_or(false))
            .or(all.first())
    } else {
        all.iter()
            .find(|m| m.id().is_ok_and(|n| n.to_string() == id))
    }
    .ok_or("Selected display is unavailable")?;
    Ok(format!(
        "{}:{:?}:{:?}:{:?}:{:?}",
        monitor.id().map_err(|e| e.to_string())?,
        monitor.width(),
        monitor.height(),
        monitor.x(),
        monitor.y()
    ))
}
pub struct ScreenCapture {
    monitor: Monitor,
    pub dimensions: (u32, u32),
    #[cfg(target_os = "windows")]
    thumbnail: crate::capture_windows::Thumbnail,
    #[cfg(target_os = "windows")]
    desktop: Option<crate::capture_windows::Desktop>,
}
impl ScreenCapture {
    pub fn start(id: &str) -> Result<Self, String> {
        check_session()?;
        let all = Monitor::all().map_err(|e| e.to_string())?;
        let monitor = if id.is_empty() {
            all.iter()
                .find(|m| m.is_primary().unwrap_or(false))
                .or(all.first())
        } else {
            all.iter()
                .find(|m| m.id().map(|v| v.to_string() == id).unwrap_or(false))
        }
        .ok_or("Selected display is unavailable. Refresh the display list.")?
        .clone();
        #[cfg(not(target_os = "windows"))]
        let image = monitor.capture_image().map_err(|e| {
            format!("Screen capture failed: {e}. Check screen recording permission.")
        })?;
        #[cfg(target_os = "windows")]
        {
            let dimensions = (
                monitor.width().map_err(|e| e.to_string())?,
                monitor.height().map_err(|e| e.to_string())?,
            );
            let mut thumbnail = crate::capture_windows::Thumbnail::new(dimensions.0, dimensions.1)?;
            thumbnail.grab(
                monitor.x().map_err(|e| e.to_string())?,
                monitor.y().map_err(|e| e.to_string())?,
                dimensions.0,
                dimensions.1,
            )?;
            Ok(Self {
                desktop: crate::capture_windows::Desktop::new(&monitor.name().unwrap_or_default())
                    .ok(),
                monitor,
                dimensions,
                thumbnail,
            })
        }
        #[cfg(not(target_os = "windows"))]
        Ok(Self {
            monitor,
            dimensions: (image.width(), image.height()),
        })
    }
    pub fn colors(&mut self, settings: &Settings) -> Result<Vec<Rgb>, String> {
        #[cfg(target_os = "windows")]
        {
            if let Some(desktop) = self.desktop.as_mut() {
                return desktop.colors(settings);
            }
            let dimensions = (
                self.monitor.width().map_err(|e| e.to_string())?,
                self.monitor.height().map_err(|e| e.to_string())?,
            );
            if self.dimensions != dimensions {
                self.thumbnail =
                    crate::capture_windows::Thumbnail::new(dimensions.0, dimensions.1)?;
                self.dimensions = dimensions;
            }
            let (w, h) = (self.thumbnail.width, self.thumbnail.height);
            let pixels = self.thumbnail.grab(
                self.monitor.x().map_err(|e| e.to_string())?,
                self.monitor.y().map_err(|e| e.to_string())?,
                dimensions.0,
                dimensions.1,
            )?;
            Ok(lighting::sample(pixels, w, h, settings))
        }
        #[cfg(not(target_os = "windows"))]
        {
            let image = self.monitor.capture_image().map_err(|e| e.to_string())?;
            self.dimensions = (image.width(), image.height());
            Ok(lighting::sample(
                image.as_raw(),
                image.width(),
                image.height(),
                settings,
            ))
        }
    }
}

fn check_session() -> Result<(), String> {
    #[cfg(target_os = "linux")]
    if crate::linux::wayland() {
        // xcap's still-image Wayland fallback may write screenshots to disk or
        // ask repeatedly. Do not use it for continuous private screen sampling.
        return Err("Screen sync currently needs an X11 session. Log in with Xorg/X11, or use Solid color, Music, or Color flow on Wayland.".into());
    }
    Ok(())
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires an X11 display; CI supplies Xvfb"]
    fn x11_capture_samples_display() {
        let mut capture = ScreenCapture::start("").unwrap();
        let settings = Settings::default();
        let colors = capture.colors(&settings).unwrap();
        assert_eq!(colors.len(), settings.count);
        assert!(capture.dimensions.0 > 0 && capture.dimensions.1 > 0);
    }
}
