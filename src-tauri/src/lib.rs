mod audio;
mod automation;
mod beat;
mod capture;
#[cfg(target_os = "windows")]
mod capture_windows;
mod compact;
mod device;
mod diagnostics;
mod effects;
mod engine;
mod lifecycle;
#[cfg(target_os = "windows")]
mod lifecycle_windows;
mod lighting;
#[cfg(target_os = "linux")]
mod linux;
mod platform;
mod profiles;
mod protocol;
mod scenes;
mod settings;
mod sharing;
mod tray;
mod updates;

use engine::{Action, Handle, Snapshot};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tauri::{Emitter, Manager};
struct Preview(Arc<AtomicBool>);

#[tauri::command]
fn launch_at_login(app: tauri::AppHandle, enabled: Option<bool>) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;
    let manager = app.autolaunch();
    if let Some(enabled) = enabled {
        if enabled {
            manager.enable()
        } else {
            manager.disable()
        }
        .map_err(|e| e.to_string())?;
    }
    manager.is_enabled().map_err(|e| e.to_string())
}

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
fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
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
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_single_instance::init(|app, args, _| {
            if !args.iter().any(|arg| arg == "--autostart") {
                show(app);
            }
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--autostart"]),
        ))
        .invoke_handler(tauri::generate_handler![
            control,
            displays,
            quit,
            platform_name,
            app_version,
            automation::automation_status,
            automation::automation_configure,
            updates::update_status,
            updates::update_check,
            updates::update_install,
            updates::update_automatic,
            effect_previews,
            launch_at_login,
            profiles::profile_library,
            profiles::apply_profile
            ,diagnostics::diagnostic_report,
            diagnostics::export_diagnostics
            ,compact::compact_window,
            compact::compact_control
            ,scenes::starter_scenes,
            scenes::apply_starter,
            scenes::scene_previews
            ,sharing::preview_profile_import,
            sharing::import_profiles,
            sharing::export_profiles
            ,sharing::remember_color
        ])
        .setup(|app| {
            let background = std::env::args().any(|arg| arg == "--autostart");
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
            let diagnostics = diagnostics::Diagnostics::new(path.with_file_name("diagnostics.json"));
            diagnostics.start_writer();
            app.manage(diagnostics.clone());
            let preview = Arc::new(AtomicBool::new(!background));
            app.manage(Preview(preview.clone()));
            let handle = engine::start(settings, path, warning, move |snapshot| {
                diagnostics.observe(snapshot);
                // Never synchronously query the GUI thread from the lighting
                // worker: the GUI may be waiting for a suspend acknowledgement.
                if preview.load(Ordering::Relaxed) {
                    let _ = emitter.emit_to("main", "lighting", snapshot);
                }
            });
            if let Err(e) = platform::watch_power(handle.clone()) {
                handle.system_event(lifecycle::SystemEvent::Warning(e), false);
                handle.system_event(lifecycle::SystemEvent::Ready, false);
            }
            app.manage(handle);
            if let Err(e) = app.handle().plugin(tauri_plugin_global_shortcut::Builder::new().build()) {
                app.state::<Handle>().system_event(lifecycle::SystemEvent::Warning(format!("Global shortcuts unavailable: {e}. Profiles can still be selected in the app or tray.")), false);
            }
            app.manage(AtomicBool::new(false));
            updates::init(app.handle()).map_err(std::io::Error::other)?;
            profiles::init(app.handle()).map_err(std::io::Error::other)?;
            automation::init(app.handle()).map_err(std::io::Error::other)?;
            tray::init(app.handle())?;
            if let Some(window) = app.get_webview_window("main") {
                if !background {
                    window.show()?;
                }
                #[cfg(target_os = "linux")]
                if background {
                    // Keep the app reachable on desktops without a tray host.
                    window.show()?;
                    window.minimize()?;
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "compact" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    if let Some(webview) = window.app_handle().get_webview_window("compact") {
                        if let Err(e) = compact::remember(&webview) { let _ = window.emit("control-error", e); }
                    }
                    let _ = window.hide();
                }
                return;
            }
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
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Reopen { .. } = event {
            show(app);
        }
        if let tauri::RunEvent::ExitRequested { api, .. } = event {
            if !app.state::<AtomicBool>().swap(true, Ordering::SeqCst) {
                automation::stop(app);
                if let Some(window) = app.get_webview_window("compact") {
                    let _ = compact::remember(&window);
                }
                api.prevent_exit();
                let handle = app.state::<Handle>().inner().clone();
                let app = app.clone();
                tauri::async_runtime::spawn_blocking(move || {
                    let _ = handle.request(Action::Shutdown);
                    #[cfg(target_os = "linux")]
                    crate::linux::stop_watchers();
                    app.exit(0);
                });
            }
        }
    });
}
