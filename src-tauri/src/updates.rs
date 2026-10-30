//! The renderer never supplies an endpoint, public key, version, or installer.
use crate::{
    engine::{Action, Handle},
    settings::save_json,
};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::Duration,
};
use tauri::{Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    phase: &'static str,
    current_version: &'static str,
    version: Option<String>,
    notes: String,
    message: String,
    automatic: bool,
    can_install: bool,
    progress: Option<u64>,
}
#[derive(Serialize, Deserialize)]
struct Preferences {
    automatic: bool,
}
struct Model {
    status: Status,
    pending: Option<Update>,
}
pub struct Updates {
    model: Mutex<Model>,
    busy: AtomicBool,
    path: PathBuf,
}
struct Operation<'a>(&'a AtomicBool);
impl<'a> Operation<'a> {
    fn begin(busy: &'a AtomicBool) -> Result<Self, String> {
        busy.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "An update operation is already running".to_string())?;
        Ok(Self(busy))
    }
}
impl Drop for Operation<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
impl Updates {
    fn begin(&self) -> Result<Operation<'_>, String> {
        Operation::begin(&self.busy)
    }
}
fn change(app: &tauri::AppHandle, edit: impl FnOnce(&mut Model)) -> Status {
    let updates = app.state::<Updates>();
    let mut model = updates.model.lock().unwrap();
    edit(&mut model);
    let status = model.status.clone();
    let _ = app.emit_to("main", "update-status", &status);
    status
}
fn main_window(window: &tauri::WebviewWindow) -> Result<(), String> {
    if window.label() == "main" {
        Ok(())
    } else {
        Err("Updates are available in the main window".into())
    }
}
fn target() -> String {
    let os = if cfg!(target_os = "macos") {
        "darwin"
    } else {
        std::env::consts::OS
    };
    format!("{os}-{}", std::env::consts::ARCH)
}
fn trusted_download(url: &tauri::Url, version: &str) -> bool {
    url.scheme() == "https"
        && url.host_str() == Some("github.com")
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && url.path().starts_with(&format!(
            "/BrenOne/DynamicLight/releases/download/v{version}/dynamiclight-desktop-"
        ))
}
pub fn init(app: &tauri::AppHandle) -> Result<(), String> {
    let path = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("update-preferences.json");
    let (automatic, message) = match std::fs::read(&path) {
        Ok(bytes) => match serde_json::from_slice::<Preferences>(&bytes) {
            Ok(p) => (p.automatic, String::new()),
            Err(_) => (
                false,
                "Update preferences could not be read. Automatic checks are disabled.".into(),
            ),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (true, String::new()),
        Err(_) => (
            false,
            "Update preferences could not be read. Automatic checks are disabled.".into(),
        ),
    };
    #[cfg(target_os = "linux")]
    let package_supported = app.env().appimage.is_some();
    #[cfg(not(target_os = "linux"))]
    let package_supported = true;
    let can_install = !cfg!(debug_assertions) && package_supported;
    app.manage(Updates {
        model: Mutex::new(Model {
            status: Status {
                phase: "idle",
                current_version: env!("CARGO_PKG_VERSION"),
                version: None,
                notes: String::new(),
                message,
                automatic,
                can_install,
                progress: None,
            },
            pending: None,
        }),
        busy: AtomicBool::new(false),
        path,
    });
    // Native timer continues when the webview is hidden in the tray. Development
    // builds never contact GitHub automatically.
    if !cfg!(debug_assertions) {
        let app = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(30));
            loop {
                let automatic = app
                    .state::<Updates>()
                    .model
                    .lock()
                    .unwrap()
                    .status
                    .automatic;
                if automatic {
                    let _ = tauri::async_runtime::block_on(check(&app));
                }
                std::thread::sleep(Duration::from_secs(6 * 60 * 60));
            }
        });
    }
    Ok(())
}
#[tauri::command]
pub fn update_status(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<Status, String> {
    main_window(&window)?;
    Ok(app.state::<Updates>().model.lock().unwrap().status.clone())
}
#[tauri::command]
pub fn update_automatic(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    enabled: bool,
) -> Result<Status, String> {
    main_window(&window)?;
    let updates = app.state::<Updates>();
    let mut model = updates.model.lock().unwrap();
    save_json(&Preferences { automatic: enabled }, &updates.path)?;
    model.status.automatic = enabled;
    let status = model.status.clone();
    let _ = app.emit_to("main", "update-status", &status);
    Ok(status)
}
async fn check(app: &tauri::AppHandle) -> Result<Status, String> {
    let updates = app.state::<Updates>();
    let _operation = updates.begin()?;
    change(app, |m| {
        m.status.phase = "checking";
        m.status.message.clear();
        m.status.progress = None;
    });
    let result = async {
        let updater = app
            .updater_builder()
            .target(target())
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(|e| e.to_string())?;
        let pending = updater.check().await.map_err(|e| e.to_string())?;
        if let Some(update) = &pending {
            if !trusted_download(&update.download_url, &update.version) {
                return Err("The release contains an unexpected download address".into());
            }
        }
        Ok::<_, String>(pending)
    }
    .await;
    Ok(change(app, |m| match result {
        Ok(pending) => {
            m.status.phase = if pending.is_some() {
                "available"
            } else {
                "current"
            };
            m.status.version = pending.as_ref().map(|p| p.version.clone());
            m.status.notes = pending
                .as_ref()
                .and_then(|p| p.body.clone())
                .unwrap_or_default();
            m.pending = pending;
        }
        Err(error) => {
            m.status.phase = "error";
            m.status.message = format!("Could not check for updates: {error}");
        }
    }))
}
#[tauri::command]
pub async fn update_check(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<Status, String> {
    main_window(&window)?;
    check(&app).await
}
#[tauri::command]
pub async fn update_install(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<Status, String> {
    main_window(&window)?;
    let updates = app.state::<Updates>();
    let _operation = updates.begin()?;
    let mut update = {
        let model = updates.model.lock().unwrap();
        if !model.status.can_install {
            return Err(
                "Install the latest package using your terminal installer or package manager"
                    .into(),
            );
        }
        model.pending.clone().ok_or("Check for an update first")?
    };
    update.timeout = Some(Duration::from_secs(15 * 60));
    change(&app, |m| {
        m.status.phase = "downloading";
        m.status.message.clear();
        m.status.progress = None;
    });
    let mut received = 0u64;
    let mut last_progress = None;
    let bytes = update
        .download(
            |chunk, total| {
                received = received.saturating_add(chunk as u64);
                let progress = total.filter(|t| *t > 0).map(|t| {
                    received
                        .saturating_mul(100)
                        .checked_div(t)
                        .unwrap_or(0)
                        .min(100)
                });
                if progress != last_progress {
                    last_progress = progress;
                    change(&app, |m| m.status.progress = progress);
                }
            },
            || {},
        )
        .await;
    let result = match bytes {
        Err(e) => Err(format!(
            "Download or signature verification failed: {e}. Lighting has not been stopped."
        )),
        Ok(bytes) => {
            change(&app, |m| {
                m.status.phase = "installing";
                m.status.progress = None;
            });
            let app = app.clone();
            tauri::async_runtime::spawn_blocking(move || {
                if let Some(window) = app.get_webview_window("compact") {
                    crate::compact::remember(&window)?;
                }
                let handle = app.state::<Handle>().inner().clone();
                // Keep the worker alive so a failed installer can be retried and
                // lighting can be started again. Preserve the saved startup intent.
                crate::automation::stop(&app);
                crate::remote::stop(&app);
                crate::game::stop(&app);
                handle.request(Action::PrepareUpdate)?;
                update.install(bytes).map_err(|e| format!("Installation failed: {e}. You can start lighting again or retry the update."))?;
                let _ = handle.request(Action::Shutdown);
                #[cfg(target_os = "linux")]
                crate::linux::stop_watchers();
                app.state::<AtomicBool>().store(true, Ordering::SeqCst);
                app.restart();
                #[allow(unreachable_code)]
                Ok::<(), String>(())
            }).await.map_err(|e| e.to_string()).and_then(|r| r)
        }
    };
    Ok(change(&app, |m| {
        m.status.phase = "error";
        m.status.message = result
            .err()
            .unwrap_or_else(|| "Restart DynamicLight to finish updating".into());
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn downloads_are_bound_to_our_versioned_release() {
        for (url, allowed) in [
            ("https://github.com/BrenOne/DynamicLight/releases/download/v0.9.0/dynamiclight-desktop-windows-x64-setup.exe", true),
            ("http://github.com/BrenOne/DynamicLight/releases/download/v0.9.0/dynamiclight-desktop-test", false),
            ("https://github.com/other/repo/releases/download/v0.9.0/dynamiclight-desktop-test", false),
            ("https://github.com/BrenOne/DynamicLight/releases/download/v0.8.1/dynamiclight-desktop-test", false),
            ("https://github.com.evil.test/BrenOne/DynamicLight/releases/download/v0.9.0/dynamiclight-desktop-test", false),
        ] { assert_eq!(trusted_download(&url.parse().unwrap(), "0.9.0"), allowed); }
    }
    #[test]
    fn operations_are_exclusive_and_released_after_failure() {
        let busy = AtomicBool::new(false);
        {
            let _operation = Operation::begin(&busy).unwrap();
            assert!(Operation::begin(&busy).is_err());
        }
        assert!(!busy.load(Ordering::Acquire));
        assert!(Operation::begin(&busy).is_ok());
    }
}
