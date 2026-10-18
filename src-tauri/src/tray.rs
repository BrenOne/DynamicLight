use crate::{
    engine::{Action, Handle, Snapshot},
    profiles::Store,
};
use tauri::{
    menu::{Menu, MenuItem, Submenu},
    tray::TrayIconBuilder,
    Emitter, Manager,
};

fn status(s: &Snapshot) -> String {
    if !s.requested {
        "Stopped".into()
    } else if !s.pause_reason.is_empty() {
        s.pause_reason.clone()
    } else if s.capture_requires_start {
        "Capture paused — Retry to continue".into()
    } else if s.recovering {
        "Reconnecting".into()
    } else if !s.error.is_empty() {
        "Needs attention — open DynamicLight".into()
    } else {
        format!("{} · {}%", s.settings.mode, s.settings.brightness)
    }
}
pub fn init(app: &tauri::AppHandle) -> tauri::Result<()> {
    TrayIconBuilder::with_id("lighting")
        .icon(app.default_window_icon().cloned().expect("app icon"))
        .tooltip("DynamicLight")
        .on_menu_event(|app, event| {
            let id = event.id.as_ref().to_owned();
            match id.as_str() {
                "open" => super::show(app),
                "quit" => app.exit(0),
                _ => {
                    let app = app.clone();
                    tauri::async_runtime::spawn_blocking(move || {
                        if let Err(e) = dispatch(&app, &id) {
                            let _ = app.emit("control-error", e);
                        }
                    });
                }
            }
        })
        .build(app)?;
    let app = app.clone();
    std::thread::spawn(move || {
        let handle = app.state::<Handle>().inner().clone();
        let mut previous = String::new();
        let mut recent: Vec<String> = vec![];
        while let Ok(s) = handle.request(Action::Snapshot) {
            if recent.first() != Some(&s.settings.color) {
                recent.retain(|c| c != &s.settings.color);
                recent.insert(0, s.settings.color.clone());
                recent.truncate(6);
            }
            let library = app.state::<Store>().library.lock().unwrap().clone();
            let signature = format!(
                "{}|{}|{}|{:?}",
                status(&s),
                s.requested,
                serde_json::to_string(&library).unwrap_or_default(),
                recent
            );
            if signature != previous {
                if let Ok(menu) = menu(&app, &s, &library, &recent) {
                    if let Some(tray) = app.tray_by_id("lighting") {
                        let _ = tray.set_menu(Some(menu));
                        let _ = tray.set_tooltip(Some(format!("DynamicLight · {}", status(&s))));
                    }
                    previous = signature;
                }
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    });
    Ok(())
}
fn menu(
    app: &tauri::AppHandle,
    s: &Snapshot,
    library: &crate::profiles::Library,
    recent: &[String],
) -> tauri::Result<Menu<tauri::Wry>> {
    let menu = Menu::new(app)?;
    for (id, label, enabled) in [
        ("open", "Open DynamicLight".to_owned(), true),
        ("status", status(s), false),
        (
            "toggle",
            if s.requested { "Stop" } else { "Start" }.into(),
            true,
        ),
        ("retry", "Retry connection / capture".into(), s.requested),
    ] {
        menu.append(&MenuItem::with_id(app, id, label, enabled, None::<&str>)?)?;
    }
    let brightness = Submenu::new(app, "Brightness", true)?;
    for n in [10, 25, 50, 75, 100] {
        brightness.append(&MenuItem::with_id(
            app,
            format!("brightness:{n}"),
            format!("{n}%"),
            true,
            None::<&str>,
        )?)?;
    }
    menu.append(&brightness)?;
    let colors = Submenu::new(app, "Recent colors", true)?;
    for color in recent {
        colors.append(&MenuItem::with_id(
            app,
            format!("color:{color}"),
            color,
            true,
            None::<&str>,
        )?)?;
    }
    menu.append(&colors)?;
    let favorites = Submenu::new(
        app,
        "Favorite profiles",
        library.profiles.iter().any(|p| p.favorite),
    )?;
    for p in library.profiles.iter().filter(|p| p.favorite) {
        favorites.append(&MenuItem::with_id(
            app,
            format!("profile:{}", p.id),
            &p.name,
            true,
            None::<&str>,
        )?)?;
    }
    menu.append(&favorites)?;
    menu.append(&MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?)?;
    Ok(menu)
}
fn dispatch(app: &tauri::AppHandle, id: &str) -> Result<(), String> {
    if let Some(id) = id.strip_prefix("profile:") {
        crate::profiles::apply(app, id)?;
        return Ok(());
    }
    let handle = app.state::<Handle>();
    let next = match id {
        "toggle" => handle.request(Action::Toggle)?,
        "retry" => handle.request(Action::Start)?,
        _ => {
            if let Some(n) = id.strip_prefix("brightness:") {
                handle.request(Action::Adjust {
                    brightness: Some(n.parse().map_err(|_| "Invalid brightness")?),
                    color: None,
                })?
            } else if let Some(color) = id.strip_prefix("color:") {
                handle.request(Action::Adjust {
                    brightness: None,
                    color: Some(color.into()),
                })?
            } else {
                return Ok(());
            }
        }
    };
    let _ = app.emit("external-settings", next);
    Ok(())
}
