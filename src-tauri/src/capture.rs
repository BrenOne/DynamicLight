use crate::{lighting, protocol::Rgb, settings::Settings};
use serde::Serialize;
use xcap::Monitor;
#[derive(Serialize)]
pub struct Screen {
    pub id: String,
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub primary: bool,
}
pub fn screens() -> Result<Vec<Screen>, String> {
    check_session()?;
    Monitor::all()
        .map_err(|e| e.to_string())?
        .iter()
        .map(|m| {
            Ok(Screen {
                x: m.x().unwrap_or(0),
                y: m.y().unwrap_or(0),
                width: m.width().unwrap_or(1920),
                height: m.height().unwrap_or(1080),
                primary: m.is_primary().unwrap_or(false),
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
pub fn source_signature(settings: &Settings) -> Result<String, String> {
    if matches!(
        settings.capture_source,
        crate::capture_source::Source::Window { .. }
    ) {
        settings.capture_source.window()?;
        return Ok(format!(
            "{}:{}",
            serde_json::to_string(&settings.capture_source).map_err(|e| e.to_string())?,
            display_aspect(&settings.screen_id)?
        ));
    }
    let id = settings.screen_id.as_str();
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
enum Capture {
    Display(Box<DisplayCapture>),
    Window {
        window: xcap::Window,
        pid: u32,
        aspect: f32,
        #[cfg(not(target_os = "windows"))]
        crop: lighting::Crop,
        #[cfg(target_os = "windows")]
        capture: Box<crate::capture_windows::WindowCapture>,
    },
}
pub struct ScreenCapture {
    inner: Capture,
    pub dimensions: (u32, u32),
}
impl ScreenCapture {
    pub fn start(settings: &Settings) -> Result<Self, String> {
        check_session()?;
        if matches!(
            settings.capture_source,
            crate::capture_source::Source::Window { .. }
        ) {
            let window = settings.capture_source.window()?;
            let dimensions = (
                window.width().map_err(|e| e.to_string())?,
                window.height().map_err(|e| e.to_string())?,
            );
            Ok(Self {
                dimensions,
                inner: Capture::Window {
                    pid: window.pid().map_err(|e| e.to_string())?,
                    aspect: display_aspect(&settings.screen_id)?,
                    window: window.clone(),
                    #[cfg(not(target_os = "windows"))]
                    crop: lighting::Crop::default(),
                    #[cfg(target_os = "windows")]
                    capture: Box::new(crate::capture_windows::WindowCapture::new(
                        window.id().map_err(|e| e.to_string())?,
                    )?),
                },
            })
        } else {
            let display = DisplayCapture::start(settings)?;
            Ok(Self {
                dimensions: display.dimensions,
                inner: Capture::Display(Box::new(display)),
            })
        }
    }
    pub fn colors(&mut self, settings: &Settings) -> Result<Vec<Rgb>, String> {
        match &mut self.inner {
            Capture::Display(display) => {
                let colors = display.colors(settings)?;
                self.dimensions = display.dimensions;
                Ok(colors)
            }
            Capture::Window {
                window,
                pid,
                aspect,
                #[cfg(not(target_os = "windows"))]
                crop,
                #[cfg(target_os = "windows")]
                capture,
            } => {
                if window.pid().ok() != Some(*pid) || window.is_minimized().unwrap_or(true) {
                    return Err(
                        "Selected window closed or minimized. Restore it and Retry capture.".into(),
                    );
                }
                self.dimensions = (
                    window.width().map_err(|e| e.to_string())?,
                    window.height().map_err(|e| e.to_string())?,
                );
                if u64::from(self.dimensions.0) * u64::from(self.dimensions.1) > 40_000_000 {
                    return Err("Window exceeds the 40 megapixel capture limit".into());
                }
                #[cfg(target_os = "windows")]
                {
                    capture.colors(settings, *aspect)
                }
                #[cfg(not(target_os = "windows"))]
                {
                    let image = window.capture_image().map_err(|e| e.to_string())?;
                    self.dimensions = (image.width(), image.height());
                    Ok(lighting::sample_geometry(
                        image.as_raw(),
                        (
                            image.width(),
                            image.height(),
                            image.width() as usize * 4,
                            false,
                        ),
                        settings,
                        crop,
                        *aspect,
                    ))
                }
            }
        }
    }
}
pub struct DisplayCapture {
    crop: lighting::Crop,
    monitor: Monitor,
    region: crate::capture_source::Source,
    pub dimensions: (u32, u32),
    #[cfg(target_os = "windows")]
    thumbnail: crate::capture_windows::Thumbnail,
    #[cfg(target_os = "windows")]
    desktop: Option<crate::capture_windows::Desktop>,
}
impl DisplayCapture {
    pub fn start(settings: &Settings) -> Result<Self, String> {
        let id = settings.screen_id.as_str();
        let region = settings.capture_source.clone();
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
        let rect = region.rectangle(
            monitor.width().map_err(|e| e.to_string())?,
            monitor.height().map_err(|e| e.to_string())?,
        )?;
        #[cfg(not(target_os = "windows"))]
        let image = monitor
            .capture_region(rect.0, rect.1, rect.2, rect.3)
            .map_err(|e| {
                format!("Screen capture failed: {e}. Check screen recording permission.")
            })?;
        #[cfg(target_os = "windows")]
        {
            let dimensions = (
                monitor.width().map_err(|e| e.to_string())?,
                monitor.height().map_err(|e| e.to_string())?,
            );
            let rect = region.rectangle(dimensions.0, dimensions.1)?;
            let dimensions = (rect.2, rect.3);
            let mut thumbnail = crate::capture_windows::Thumbnail::new(dimensions.0, dimensions.1)?;
            thumbnail.grab(
                monitor.x().map_err(|e| e.to_string())? + rect.0 as i32,
                monitor.y().map_err(|e| e.to_string())? + rect.1 as i32,
                dimensions.0,
                dimensions.1,
            )?;
            Ok(Self {
                desktop: if matches!(region, crate::capture_source::Source::Display {}) {
                    crate::capture_windows::Desktop::new(&monitor.name().unwrap_or_default()).ok()
                } else {
                    None
                },
                crop: lighting::Crop::default(),
                monitor,
                region,
                dimensions,
                thumbnail,
            })
        }
        #[cfg(not(target_os = "windows"))]
        Ok(Self {
            crop: lighting::Crop::default(),
            monitor,
            region,
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
            let rect = self.region.rectangle(dimensions.0, dimensions.1)?;
            let dimensions = (rect.2, rect.3);
            if self.dimensions != dimensions {
                self.thumbnail =
                    crate::capture_windows::Thumbnail::new(dimensions.0, dimensions.1)?;
                self.dimensions = dimensions;
            }
            let (w, h) = (self.thumbnail.width, self.thumbnail.height);
            let pixels = self.thumbnail.grab(
                self.monitor.x().map_err(|e| e.to_string())? + rect.0 as i32,
                self.monitor.y().map_err(|e| e.to_string())? + rect.1 as i32,
                dimensions.0,
                dimensions.1,
            )?;
            Ok(lighting::sample_geometry(
                pixels,
                (w, h, w as usize * 4, false),
                settings,
                &mut self.crop,
                self.monitor.width().unwrap_or(16) as f32
                    / self.monitor.height().unwrap_or(9).max(1) as f32,
            ))
        }
        #[cfg(not(target_os = "windows"))]
        {
            let rect = self.region.rectangle(
                self.monitor.width().map_err(|e| e.to_string())?,
                self.monitor.height().map_err(|e| e.to_string())?,
            )?;
            let image = self
                .monitor
                .capture_region(rect.0, rect.1, rect.2, rect.3)
                .map_err(|e| e.to_string())?;
            self.dimensions = (image.width(), image.height());
            Ok(lighting::sample_geometry(
                image.as_raw(),
                (
                    image.width(),
                    image.height(),
                    image.width() as usize * 4,
                    false,
                ),
                settings,
                &mut self.crop,
                self.monitor.width().unwrap_or(16) as f32
                    / self.monitor.height().unwrap_or(9).max(1) as f32,
            ))
        }
    }
}

fn display_aspect(id: &str) -> Result<f32, String> {
    let monitors = Monitor::all().map_err(|e| e.to_string())?;
    let monitor = if id.is_empty() {
        monitors
            .iter()
            .find(|m| m.is_primary().unwrap_or(false))
            .or(monitors.first())
    } else {
        monitors
            .iter()
            .find(|m| m.id().is_ok_and(|n| n.to_string() == id))
    }
    .ok_or("Select the physical display for this strip")?;
    Ok(monitor.width().map_err(|e| e.to_string())? as f32
        / monitor.height().map_err(|e| e.to_string())?.max(1) as f32)
}
pub(crate) fn check_session() -> Result<(), String> {
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
        let mut capture = ScreenCapture::start(&Settings::default()).unwrap();
        let settings = Settings::default();
        let colors = capture.colors(&settings).unwrap();
        assert_eq!(colors.len(), settings.count);
        assert!(capture.dimensions.0 > 0 && capture.dimensions.1 > 0);
    }
}
