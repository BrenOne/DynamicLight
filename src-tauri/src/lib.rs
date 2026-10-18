mod audio;
mod capture;
#[cfg(target_os = "windows")]
mod capture_windows;
mod device;
mod effects;
mod engine;
mod lighting;
#[cfg(target_os = "linux")]
mod linux;
mod platform;
mod protocol;
mod settings;

use engine::{Action, Handle, Snapshot};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Emitter, Manager,
};
struct Preview(Arc<AtomicBool>);

#[cfg(test)]
mod preview_tests {
    use super::*;
    #[test]
    fn thumbnails_use_native_effects_and_validate_input() {
        let s = settings::Settings {
            brightness: 100,
            palette: "ocean".into(),
            ..Default::default()
        };
        let frames = effect_previews(s.clone(), 2.0).unwrap();
        for (i, effect) in ["rainbow", "aurora", "breath", "comet", "ember"]
            .iter()
            .enumerate()
        {
            let expected = effects::animated(
                &settings::Settings {
                    count: 48,
                    effect: (*effect).into(),
                    ..s.clone()
                },
                2.0,
            );
            assert_eq!(frames[i], expected);
        }
        assert!(effect_previews(s.clone(), f32::NAN).is_err());
        assert!(effect_previews(s.clone(), -1.0).is_err());
        assert!(effect_previews(
            settings::Settings {
                count: 128,
                ..s.clone()
            },
            0.0
        )
        .is_err());
        assert!(
            effect_previews(settings::Settings { brightness: 0, ..s }, 2.0)
                .unwrap()
                .iter()
                .flatten()
                .all(|c| *c == [0; 3])
        );
    }
}

#[tauri::command]
fn platform_name() -> &'static str {
    std::env::consts::OS
}
#[tauri::command]
fn effect_previews(
    settings: settings::Settings,
    seconds: f32,
) -> Result<Vec<Vec<protocol::Rgb>>, String> {
    settings.validate()?;
    if !seconds.is_finite() || !(0.0..=86400.0).contains(&seconds) {
        return Err("Invalid preview time".into());
    }
    Ok(["rainbow", "aurora", "breath", "comet", "ember"]
        .iter()
        .map(|name| {
            let s = settings::Settings {
                effect: (*name).into(),
                count: 48,
                ..settings.clone()
            };
            effects::animated(&s, seconds)
                .iter()
                .map(|rgb| rgb.map(|c| (c as f32 * s.brightness as f32 / 100.0).round() as u8))
                .collect()
        })
        .collect())
}

#[tauri::command]
async fn control(action: Action, state: tauri::State<'_, Handle>) -> Result<Snapshot, String> {
    let handle = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || handle.request(action))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn displays() -> Result<Vec<capture::Screen>, String> {
    tauri::async_runtime::spawn_blocking(capture::screens)
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
fn quit(app: tauri::AppHandle) {
    app.exit(0);
}
fn show(app: &tauri::AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
}
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| show(app)))
        .invoke_handler(tauri::generate_handler![
            control,
            displays,
            quit,
            platform_name,
            effect_previews
        ])
        .setup(|app| {
            #[cfg(target_os = "windows")]
            platform::set_taskbar_icon(
                &app.get_webview_window("main")
                    .ok_or("Missing main window")?,
            )
            .map_err(std::io::Error::other)?;
            let path = app.path().app_data_dir()?.join("settings.json");
            let legacy = app
                .path()
                .config_dir()?
                .join("dynamiclight")
                .join("settings.json");
            let mut warning = String::new();
            let settings = if path.exists() {
                settings::Settings::load(&path).unwrap_or_else(|e| {
                    warning = format!("Saved settings could not be loaded: {e}");
                    settings::Settings::default()
                })
            } else if legacy.exists() {
                match settings::Settings::load(&legacy) {
                    Ok(mut s) => {
                        // Electron display identifiers do not identify native monitors.
                        s.screen_id.clear();
                        if let Err(e) = s.save(&path) {
                            warning = e;
                        }
                        s
                    }
                    Err(_) => settings::Settings::default(),
                }
            } else {
                settings::Settings::default()
            };
            let emitter = app.handle().clone();
            let preview = Arc::new(AtomicBool::new(true));
            app.manage(Preview(preview.clone()));
            let handle = engine::start(settings, path, warning, move |snapshot| {
                // Never synchronously query the GUI thread from the lighting
                // worker: the GUI may be waiting for a suspend acknowledgement.
                if preview.load(Ordering::Relaxed) {
                    let _ = emitter.emit_to("main", "lighting", snapshot);
                }
            });
            platform::watch_power(handle.clone()).map_err(std::io::Error::other)?;
            app.manage(handle);
            app.manage(AtomicBool::new(false));
            let open = MenuItem::with_id(app, "open", "Open DynamicLight", true, None::<&str>)?;
            let off = MenuItem::with_id(app, "off", "Turn lights off", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &off, &quit])?;
            TrayIconBuilder::new()
                .icon(
                    app.default_window_icon()
                        .cloned()
                        .ok_or("Missing app icon")?,
                )
                .tooltip("DynamicLight")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => show(app),
                    "off" => {
                        let handle = app.state::<Handle>().inner().clone();
                        tauri::async_runtime::spawn_blocking(move || {
                            let _ = handle.request(Action::Power { on: false });
                        });
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Focused(focused) = event {
                if let Some(preview) = window.try_state::<Preview>() {
                    preview.0.store(*focused, Ordering::Relaxed);
                }
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if let Some(preview) = window.try_state::<Preview>() {
                    preview.0.store(false, Ordering::Relaxed);
                }
                api.prevent_close();
                // GNOME and some other Linux desktops have no tray host.
                // Keep a taskbar entry so the window is always recoverable.
                #[cfg(target_os = "linux")]
                let _ = window.minimize();
                #[cfg(not(target_os = "linux"))]
                let _ = window.hide();
            }
        })
        .build(tauri::generate_context!())
        .expect("Unable to start DynamicLight");
    app.run(|app, event| {
        if let tauri::RunEvent::ExitRequested { api, .. } = event {
            if !app.state::<AtomicBool>().swap(true, Ordering::SeqCst) {
                api.prevent_exit();
                let handle = app.state::<Handle>().inner().clone();
                let app = app.clone();
                tauri::async_runtime::spawn_blocking(move || {
                    let _ = handle.request(Action::Shutdown);
                    app.exit(0);
                });
            }
        }
    });
}
