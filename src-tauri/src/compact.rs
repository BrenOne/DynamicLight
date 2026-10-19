use crate::{
    engine::{Action, Handle, Snapshot},
    settings::save_json,
};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

#[derive(Default, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Preferences {
    x: i32,
    y: i32,
    placed: bool,
    pinned: bool,
}
fn load(app: &tauri::AppHandle) -> Preferences {
    app.path()
        .app_data_dir()
        .ok()
        .and_then(|dir| std::fs::read(dir.join("compact.json")).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}
pub fn remember(window: &WebviewWindow) -> Result<(), String> {
    let pos = window.outer_position().ok();
    let prefs = Preferences {
        x: pos.map_or(0, |p| p.x),
        y: pos.map_or(0, |p| p.y),
        placed: pos.is_some(),
        pinned: window.is_always_on_top().map_err(|e| e.to_string())?,
    };
    save_json(
        &prefs,
        &window
            .app_handle()
            .path()
            .app_data_dir()
            .map_err(|e| e.to_string())?
            .join("compact.json"),
    )
}
fn clamp_position(
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    area: (i32, i32, u32, u32),
) -> (i32, i32) {
    let (left, top, w, h) = area;
    let right = (left as i64 + w.saturating_sub(width) as i64)
        .clamp(i32::MIN as i64, i32::MAX as i64) as i32;
    let bottom = (top as i64 + h.saturating_sub(height) as i64)
        .clamp(i32::MIN as i64, i32::MAX as i64) as i32;
    (x.clamp(left, right), y.clamp(top, bottom))
}
fn place(window: &WebviewWindow, prefs: &Preferences) -> Result<(), String> {
    // Wayland compositors may withhold placement APIs; let them place the window.
    let monitors = window.available_monitors().unwrap_or_default();
    let selected = monitors
        .iter()
        .find(|m| {
            let a = m.work_area();
            prefs.placed
                && prefs.x as i64 >= a.position.x as i64
                && prefs.y as i64 >= a.position.y as i64
                && (prefs.x as i64) < a.position.x as i64 + a.size.width as i64
                && (prefs.y as i64) < a.position.y as i64 + a.size.height as i64
        })
        .cloned()
        .or(window.primary_monitor().ok().flatten())
        .or_else(|| monitors.first().cloned());
    if let Some(monitor) = selected {
        let area = monitor.work_area();
        let mut size = window.outer_size().map_err(|e| e.to_string())?;
        if size.width > area.size.width || size.height > area.size.height {
            let inner = window.inner_size().map_err(|e| e.to_string())?;
            let border = (
                size.width.saturating_sub(inner.width),
                size.height.saturating_sub(inner.height),
            );
            // A smaller replacement monitor can be below the usual logical minimum.
            window
                .set_min_size(None::<tauri::PhysicalSize<u32>>)
                .map_err(|e| e.to_string())?;
            window
                .set_size(tauri::PhysicalSize::new(
                    inner
                        .width
                        .min(area.size.width.saturating_sub(border.0).max(1)),
                    inner
                        .height
                        .min(area.size.height.saturating_sub(border.1).max(1)),
                ))
                .map_err(|e| e.to_string())?;
            size = window.outer_size().map_err(|e| e.to_string())?;
        }
        let (x, y) = clamp_position(
            if prefs.placed {
                prefs.x
            } else {
                area.position.x + 24
            },
            if prefs.placed {
                prefs.y
            } else {
                area.position.y + 24
            },
            size.width,
            size.height,
            (
                area.position.x,
                area.position.y,
                area.size.width,
                area.size.height,
            ),
        );
        // Some compositors expose monitors but still reserve window placement.
        // Keep controls reachable using the compositor's chosen position.
        let _ = window.set_position(PhysicalPosition::new(x, y));
    }
    Ok(())
}
pub fn open(app: &tauri::AppHandle) -> Result<(), String> {
    let mut prefs = load(app);
    let window = if let Some(window) = app.get_webview_window("compact") {
        if let Ok(position) = window.outer_position() {
            prefs.x = position.x;
            prefs.y = position.y;
            prefs.placed = true;
        }
        window
    } else {
        let window =
            WebviewWindowBuilder::new(app, "compact", WebviewUrl::App("compact.html".into()))
                .title("DynamicLight · Quick controls")
                .inner_size(430.0, 550.0)
                .min_inner_size(300.0, 360.0)
                .visible(false)
                .always_on_top(prefs.pinned)
                .build()
                .map_err(|e| e.to_string())?;
        #[cfg(target_os = "windows")]
        crate::platform::set_taskbar_icon(&window)?;
        window
    };
    place(&window, &prefs)?;
    window.show().map_err(|e| e.to_string())?;
    window.unminimize().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())?;
    Ok(())
}
#[tauri::command]
pub async fn compact_window(
    app: tauri::AppHandle,
    operation: String,
    pinned: Option<bool>,
) -> Result<bool, String> {
    match operation.as_str() {
        "open" => {
            open(&app)?;
        }
        "studio" => super::show(&app),
        "hide" => {
            if let Some(window) = app.get_webview_window("compact") {
                remember(&window)?;
                window.hide().map_err(|e| e.to_string())?;
            }
        }
        "pin" => {
            let window = app
                .get_webview_window("compact")
                .ok_or("Quick controls are not open")?;
            let previous = window.is_always_on_top().map_err(|e| e.to_string())?;
            window
                .set_always_on_top(pinned.ok_or("Missing pin setting")?)
                .map_err(|e| e.to_string())?;
            if let Err(e) = remember(&window) {
                let _ = window.set_always_on_top(previous);
                return Err(e);
            }
        }
        "get" => {}
        _ => return Err("Unknown window operation".into()),
    }
    Ok(app
        .get_webview_window("compact")
        .and_then(|w| w.is_always_on_top().ok())
        .unwrap_or(false))
}
#[tauri::command]
pub async fn compact_control(
    app: tauri::AppHandle,
    brightness: Option<u8>,
    toggle: bool,
    preview: Option<bool>,
    mode: Option<String>,
) -> Result<Snapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let action = if let Some(mode) = mode {
            Action::StartMode { mode }
        } else if preview.unwrap_or(false) {
            Action::PreviewBrightness {
                brightness: brightness.ok_or("Missing brightness")?,
            }
        } else if toggle {
            Action::Toggle
        } else {
            Action::Adjust {
                brightness,
                color: None,
            }
        };
        let state = app.state::<Handle>().request(action)?;
        let _ = app.emit("external-settings", &state);
        Ok(state)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn placement_handles_removed_screens_negative_coordinates_and_oversized_windows() {
        assert_eq!(
            clamp_position(5000, 4000, 360, 460, (0, 0, 1920, 1080)),
            (1560, 620)
        );
        assert_eq!(
            clamp_position(-1700, 900, 360, 460, (-1920, 0, 1920, 1080)),
            (-1700, 620)
        );
        assert_eq!(
            clamp_position(-2000, -500, 360, 460, (0, 0, 240, 240)),
            (0, 0)
        );
    }
}
