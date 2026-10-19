//! Local capture bindings. Window identities expire when this app restarts.
use serde::{Deserialize, Serialize};
use xcap::Window;

static SESSION: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    )
});

/// Coordinates relative to a source, in thousandths of its width and height.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rectangle {
    pub left: u16,
    pub top: u16,
    pub width: u16,
    pub height: u16,
}
impl Rectangle {
    fn validate(&self) -> Result<(), String> {
        if self.width == 0
            || self.height == 0
            || u32::from(self.left) + u32::from(self.width) > 1000
            || u32::from(self.top) + u32::from(self.height) > 1000
        {
            return Err("Choose a nonempty rectangle inside the capture source".into());
        }
        Ok(())
    }
    fn pixels(&self, width: u32, height: u32) -> (u32, u32, u32, u32) {
        let x = u64::from(width) * u64::from(self.left) / 1000;
        let y = u64::from(height) * u64::from(self.top) / 1000;
        let right = (u64::from(width) * u64::from(self.left + self.width)).div_ceil(1000);
        let bottom = (u64::from(height) * u64::from(self.top + self.height)).div_ceil(1000);
        (x as u32, y as u32, (right - x) as u32, (bottom - y) as u32)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum Source {
    Display {},
    Region {
        left: u16,
        top: u16,
        width: u16,
        height: u16,
    },
    Window {
        id: u32,
        pid: u32,
        app: String,
        session: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        crop: Option<Rectangle>,
    },
}
impl Default for Source {
    fn default() -> Self {
        Self::Display {}
    }
}
impl Source {
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Region {
                left,
                top,
                width,
                height,
            } => Rectangle {
                left: *left,
                top: *top,
                width: *width,
                height: *height,
            }
            .validate(),
            Self::Window {
                id,
                pid,
                app,
                session,
                crop,
            } => {
                if *id == 0 || *pid == 0 || app.len() > 500 || session.len() > 80 {
                    return Err("Invalid selected window".into());
                }
                crop.as_ref().map_or(Ok(()), Rectangle::validate)
            }
            _ => Ok(()),
        }
    }
    pub fn rectangle(&self, width: u32, height: u32) -> Result<(u32, u32, u32, u32), String> {
        self.validate()?;
        if width == 0 || height == 0 {
            return Err("Source has no pixels".into());
        }
        Ok(match self {
            Self::Region {
                left,
                top,
                width: w,
                height: h,
            } => Rectangle {
                left: *left,
                top: *top,
                width: *w,
                height: *h,
            }
            .pixels(width, height),
            Self::Window {
                crop: Some(crop), ..
            } => crop.pixels(width, height),
            _ => (0, 0, width, height),
        })
    }
    pub fn window(&self) -> Result<Window, String> {
        let Self::Window {
            id,
            pid,
            app,
            session,
            ..
        } = self
        else {
            return Err("Choose an application window".into());
        };
        if session != &*SESSION {
            return Err("Select the window again after restarting DynamicLight".into());
        }
        let window = Window::all()
            .map_err(|e| e.to_string())?
            .into_iter()
            .find(|w| {
                w.id().ok() == Some(*id)
                    && w.pid().ok() == Some(*pid)
                    && w.app_name().ok().as_ref() == Some(app)
            })
            .ok_or("Selected window closed or changed. Choose it again.")?;
        if window.is_minimized().unwrap_or(true) {
            return Err("Selected window is minimized. Restore it, then Retry capture.".into());
        }
        Ok(window)
    }
}
#[derive(Serialize)]
pub struct Choice {
    pub name: String,
    pub source: Source,
}
pub fn windows() -> Result<Vec<Choice>, String> {
    crate::capture::check_session()?;
    let mut choices = Vec::new();
    for window in Window::all().map_err(|e| e.to_string())? {
        let (Ok(id), Ok(pid), Ok(app)) = (window.id(), window.pid(), window.app_name()) else {
            continue;
        };
        if pid == std::process::id() || window.is_minimized().unwrap_or(true) || id == 0 || pid == 0
        {
            continue;
        }
        let title = window.title().unwrap_or_default();
        choices.push(Choice {
            name: format!("{} — {}", app, title.chars().take(120).collect::<String>()),
            source: Source::Window {
                id,
                pid,
                app,
                session: SESSION.clone(),
                crop: None,
            },
        });
        if choices.len() >= 200 {
            break;
        }
    }
    Ok(choices)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_window_sources_and_display_regions_keep_their_format() {
        let legacy = serde_json::json!({
            "kind": "window", "id": 1, "pid": 2, "app": "browser", "session": "local"
        });
        let source: Source = serde_json::from_value(legacy.clone()).unwrap();
        assert_eq!(source.rectangle(1920, 1080).unwrap(), (0, 0, 1920, 1080));
        assert_eq!(serde_json::to_value(source).unwrap(), legacy);
        let region = serde_json::json!({
            "kind": "region", "left": 250, "top": 0, "width": 500, "height": 1000
        });
        let source: Source = serde_json::from_value(region.clone()).unwrap();
        assert_eq!(serde_json::to_value(source).unwrap(), region);
    }
    #[test]
    fn window_crops_are_bounded_and_scale_with_the_window() {
        let mut source = Source::Window {
            id: 1,
            pid: 2,
            app: "browser".into(),
            session: "local".into(),
            crop: Some(Rectangle {
                left: 333,
                top: 250,
                width: 667,
                height: 750,
            }),
        };
        for (w, h) in [(1, 1), (1920, 1080), (3840, 2160)] {
            let (x, y, cw, ch) = source.rectangle(w, h).unwrap();
            assert_eq!((x + cw, y + ch), (w, h));
            assert!(cw > 0 && ch > 0);
        }
        let Source::Window { crop, .. } = &mut source else {
            unreachable!()
        };
        *crop = Some(Rectangle {
            left: 999,
            top: 0,
            width: 2,
            height: 1000,
        });
        assert!(source.validate().is_err());
        let Source::Window { crop, .. } = &mut source else {
            unreachable!()
        };
        *crop = Some(Rectangle {
            left: 0,
            top: 0,
            width: 1000,
            height: 0,
        });
        assert!(source.validate().is_err());
        let Source::Window { crop, .. } = &mut source else {
            unreachable!()
        };
        *crop = Some(Rectangle {
            left: u16::MAX,
            top: 0,
            width: u16::MAX,
            height: 1000,
        });
        assert!(source.validate().is_err());
    }
    #[test]
    fn region_rounding_is_bounded_at_all_display_scales() {
        for (w, h) in [(1, 1), (1920, 1080), (3840, 2160)] {
            let source = Source::Region {
                left: 333,
                top: 250,
                width: 667,
                height: 750,
            };
            let (x, y, cw, ch) = source.rectangle(w, h).unwrap();
            assert_eq!(x + cw, w);
            assert_eq!(y + ch, h);
            assert!(cw > 0 && ch > 0);
        }
        assert!(Source::Region {
            left: 999,
            top: 0,
            width: 2,
            height: 1000
        }
        .validate()
        .is_err());
        assert!(Source::Region {
            left: 0,
            top: 0,
            width: 0,
            height: 1000
        }
        .validate()
        .is_err());
        assert!(Source::Window {
            id: 1,
            pid: 1,
            app: "app".into(),
            session: "old-session".into(),
            crop: None,
        }
        .window()
        .unwrap_err()
        .contains("again"));
    }
}
