//! Portable scene files and saved colors. Importing never starts lighting.
use crate::{
    profiles::{replace_library, Library, Profile, Store},
    settings::{save_json, SceneSettings, SourceBinding},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::Manager;

pub const MAX_FILE_BYTES: usize = 512 * 1024;
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Palette {
    pub id: String,
    pub name: String,
    pub colors: Vec<String>,
}
pub fn valid_color(c: &str) -> bool {
    c.len() == 7 && c.starts_with('#') && c.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
}
fn valid_name(name: &str) -> bool {
    !name.trim().is_empty() && name.chars().count() <= 50 && !name.chars().any(char::is_control)
}
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
}
pub fn validate_colors(palettes: &[Palette], recent: &[String]) -> Result<(), String> {
    if palettes.len() > 24 || recent.len() > 12 || recent.iter().any(|c| !valid_color(c)) {
        return Err("Keep up to 24 palettes and 12 recent colors".into());
    }
    let mut ids = HashSet::new();
    let mut names = HashSet::new();
    for p in palettes {
        if !valid_id(&p.id)
            || !valid_name(&p.name)
            || !ids.insert(&p.id)
            || !names.insert(p.name.to_lowercase())
            || p.colors.is_empty()
            || p.colors.len() > 8
            || p.colors.iter().any(|c| !valid_color(c))
        {
            return Err("Palettes need unique IDs/names and 1–8 hex colors".into());
        }
    }
    Ok(())
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortableProfile {
    pub id: String,
    pub name: String,
    pub settings: SceneSettings,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bundle {
    pub format: String,
    pub version: u32,
    pub profiles: Vec<PortableProfile>,
    #[serde(default)]
    pub palettes: Vec<Palette>,
}
impl Bundle {
    fn validate(&self) -> Result<(), String> {
        if self.format != "dynamiclight" || self.version != 1 {
            return Err("Unsupported scene file format or version".into());
        }
        if self.profiles.is_empty() && self.palettes.is_empty() {
            return Err("The scene file is empty".into());
        }
        let mut ids = HashSet::new();
        if self.profiles.len() > 40 {
            return Err("A scene file can contain up to 40 profiles".into());
        }
        for p in &self.profiles {
            if !valid_id(&p.id) || !valid_name(&p.name) || !ids.insert(&p.id) {
                return Err(
                    "Scene IDs must be unique and names must contain 1–50 characters".into(),
                );
            }
            p.settings.validate()?;
        }
        validate_colors(&self.palettes, &[])
    }
}
fn parse(text: &str) -> Result<Bundle, String> {
    if text.len() > MAX_FILE_BYTES {
        return Err("Scene files must be smaller than 512 KiB".into());
    }
    let value: serde_json::Value = serde_json::from_str(text.trim_start_matches('\u{feff}'))
        .map_err(|e| format!("Invalid JSON: {e}"))?;
    let bundle = if value.get("version").is_some() || value.get("format").is_some() {
        serde_json::from_value(value).map_err(|e| format!("Invalid scene file: {e}"))?
    } else {
        // Migration from local 0.6 profile-library backups. Strip all local data.
        let legacy: Library = serde_json::from_value(value)
            .map_err(|e| format!("Invalid legacy profile file: {e}"))?;
        legacy.validate()?;
        portable(
            &legacy,
            &legacy
                .profiles
                .iter()
                .map(|p| p.id.clone())
                .collect::<Vec<_>>(),
            &legacy
                .palettes
                .iter()
                .map(|p| p.id.clone())
                .collect::<Vec<_>>(),
        )?
    };
    bundle.validate()?;
    Ok(bundle)
}
fn portable(library: &Library, ids: &[String], palette_ids: &[String]) -> Result<Bundle, String> {
    let mut profiles = Vec::new();
    let mut palettes = Vec::new();
    for id in ids {
        let p = library
            .profiles
            .iter()
            .find(|p| &p.id == id)
            .ok_or("A selected profile no longer exists")?;
        profiles.push(PortableProfile {
            id: p.id.clone(),
            name: p.name.clone(),
            settings: p.settings.clone(),
        });
    }
    for id in palette_ids {
        palettes.push(
            library
                .palettes
                .iter()
                .find(|p| &p.id == id)
                .ok_or("A selected palette no longer exists")?
                .clone(),
        );
    }
    let bundle = Bundle {
        format: "dynamiclight".into(),
        version: 1,
        profiles,
        palettes,
    };
    bundle.validate()?;
    Ok(bundle)
}
fn conflict<'a>(library: &'a Library, p: &PortableProfile) -> Option<&'a Profile> {
    library.profiles.iter().find(|o| o.id == p.id).or_else(|| {
        library
            .profiles
            .iter()
            .find(|o| o.name.to_lowercase() == p.name.to_lowercase())
    })
}
#[derive(Serialize)]
pub struct ImportPreview {
    pub bundle: Bundle,
    pub base: Library,
    pub conflicts: Vec<Option<String>>,
    pub palette_conflicts: Vec<Option<String>>,
}
#[tauri::command]
pub async fn preview_profile_import(
    app: tauri::AppHandle,
    text: String,
) -> Result<ImportPreview, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let bundle = parse(&text)?;
        let base = app
            .state::<Store>()
            .library
            .lock()
            .map_err(|e| e.to_string())?
            .clone();
        let conflicts = bundle
            .profiles
            .iter()
            .map(|p| conflict(&base, p).map(|o| o.name.clone()))
            .collect();
        let palette_conflicts = bundle
            .palettes
            .iter()
            .map(|p| {
                base.palettes
                    .iter()
                    .find(|o| o.id == p.id || o.name.to_lowercase() == p.name.to_lowercase())
                    .map(|o| o.name.clone())
            })
            .collect();
        Ok(ImportPreview {
            bundle,
            base,
            conflicts,
            palette_conflicts,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
fn unique_name(name: &str, exists: impl Fn(&str) -> bool) -> String {
    if !exists(name) {
        return name.into();
    }
    for n in 2..100 {
        let suffix = format!(" ({n})");
        let candidate = format!(
            "{}{}",
            name.chars().take(50 - suffix.len()).collect::<String>(),
            suffix
        );
        if !exists(&candidate) {
            return candidate;
        }
    }
    unreachable!("Library is bounded to 40 items")
}
fn merge(base: &Library, bundle: &Bundle, policy: &str) -> Result<Library, String> {
    bundle.validate()?;
    if !["copy", "replace", "skip"].contains(&policy) {
        return Err("Unknown import conflict action".into());
    }
    let mut next = base.clone();
    let mut replaced = HashSet::new();
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    for (i, p) in bundle.profiles.iter().enumerate() {
        let old = conflict(base, p);
        if old.is_some() && policy == "skip" {
            continue;
        }
        if let Some(old) = old.filter(|_| policy == "replace") {
            if !replaced.insert(old.id.clone()) {
                return Err(
                    "Several imported scenes match the same local scene. Choose Keep both instead."
                        .into(),
                );
            }
            if base
                .profiles
                .iter()
                .any(|o| o.id != old.id && o.name.to_lowercase() == p.name.to_lowercase())
            {
                return Err("The imported scene's name also matches another local scene. Choose Keep both instead.".into());
            }
            let target = next
                .profiles
                .iter_mut()
                .find(|o| o.id == old.id)
                .ok_or("Profile changed")?;
            // Retain shortcut, favorite, ID and machine-specific display binding.
            target.name = p.name.clone();
            target.settings = p.settings.clone();
        } else {
            next.profiles.push(Profile {
                id: format!("import-{stamp}-{i}"),
                name: unique_name(&p.name, |n| {
                    next.profiles
                        .iter()
                        .any(|o| o.name.to_lowercase() == n.to_lowercase())
                }),
                settings: p.settings.clone(),
                source: SourceBinding::default(),
                shortcut: String::new(),
                favorite: false,
            });
        }
    }
    let mut replaced_palettes = HashSet::new();
    for (i, p) in bundle.palettes.iter().enumerate() {
        let old = base
            .palettes
            .iter()
            .find(|o| o.id == p.id || o.name.to_lowercase() == p.name.to_lowercase());
        if old.is_some() && policy == "skip" {
            continue;
        }
        if let Some(old) = old.filter(|_| policy == "replace") {
            if !replaced_palettes.insert(old.id.clone()) {
                return Err("Several imported palettes match the same local palette. Choose Keep both instead.".into());
            }
            let target = next
                .palettes
                .iter_mut()
                .find(|o| o.id == old.id)
                .ok_or("Palette changed")?;
            target.name = p.name.clone();
            target.colors = p.colors.clone();
        } else {
            next.palettes.push(Palette {
                id: format!("palette-{stamp}-{i}"),
                name: unique_name(&p.name, |n| {
                    next.palettes
                        .iter()
                        .any(|o| o.name.to_lowercase() == n.to_lowercase())
                }),
                colors: p.colors.clone(),
            });
        }
    }
    next.validate()?;
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;
    fn library() -> Library {
        Library {
            profiles: vec![Profile {
                id: "desk".into(),
                name: "Desk".into(),
                favorite: true,
                shortcut: "Control+Shift+D".into(),
                source: SourceBinding {
                    screen_id: "private-screen-path".into(),
                },
                settings: SceneSettings::capture(&Settings {
                    custom_palette: vec!["#123456".into(), "#abcdef".into()],
                    palette: "custom".into(),
                    ..Default::default()
                }),
            }],
            palettes: vec![Palette {
                id: "test".into(),
                name: "Test".into(),
                colors: vec!["#123456".into()],
            }],
            recent_colors: vec!["#abcdef".into()],
        }
    }
    #[test]
    fn equally_named_saved_scenes_can_be_exported_and_imported_as_distinct_copies() {
        let mut base = library();
        let mut second = base.profiles[0].clone();
        second.id = "second".into();
        second.shortcut.clear();
        base.profiles.push(second);
        let bundle = portable(&base, &["desk".into(), "second".into()], &[]).unwrap();
        let imported = merge(&Library::default(), &bundle, "copy").unwrap();
        assert_eq!(imported.profiles.len(), 2);
        assert_eq!(imported.profiles[0].name, "Desk");
        assert_eq!(imported.profiles[1].name, "Desk (2)");
        assert!(merge(&base, &bundle, "replace").is_err());
    }
    #[test]
    fn several_palette_matches_cannot_silently_replace_one_target() {
        let base = library();
        let mut bundle = portable(&base, &[], &["test".into()]).unwrap();
        bundle.palettes[0].name = "First".into();
        bundle.palettes.push(Palette {
            id: "second".into(),
            name: "Test".into(),
            colors: vec!["#ff0000".into()],
        });
        let error = merge(&base, &bundle, "replace").err().unwrap();
        assert!(error.contains("same local palette"));
        assert_eq!(merge(&base, &bundle, "copy").unwrap().palettes.len(), 3);
    }
    #[test]
    fn backup_preserves_the_original_library_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("profiles.json");
        std::fs::write(&path, b"an old file that failed to decode").unwrap();
        backup_library(&path, &Library::default()).unwrap();
        assert_eq!(
            std::fs::read(path.with_file_name("profiles.before-import.json")).unwrap(),
            b"an old file that failed to decode"
        );
    }
    #[test]
    fn selected_export_roundtrips_without_local_state_and_creates_missing_directory() {
        let base = library();
        let bundle = portable(&base, &["desk".into()], &["test".into()]).unwrap();
        let text = serde_json::to_string(&bundle).unwrap();
        for private in [
            "private-screen-path",
            "shortcut",
            "favorite",
            "source",
            "recent_colors",
            "count",
            "restoreOnStart",
            "maxBrightness",
        ] {
            assert!(!text.contains(private), "Leaked {private}");
        }
        let dir = tempfile::tempdir().unwrap();
        let path = write_export(&dir.path().join("missing/Downloads"), &bundle).unwrap();
        let decoded = parse(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(decoded.profiles[0].settings, base.profiles[0].settings);
        assert_eq!(decoded.palettes[0].colors, base.palettes[0].colors);
        assert!(portable(&base, &["removed".into()], &[]).is_err());
    }
    #[test]
    fn imports_validate_conflicts_and_preserve_machine_bindings() {
        let base = library();
        let mut bundle = portable(&base, &["desk".into()], &["test".into()]).unwrap();
        bundle.profiles[0].settings.brightness = 71;
        let copied = merge(&base, &bundle, "copy").unwrap();
        assert_eq!(copied.profiles.len(), 2);
        assert_eq!(copied.profiles[1].name, "Desk (2)");
        assert!(copied.profiles[1].shortcut.is_empty());
        assert!(copied.profiles[1].source.screen_id.is_empty());
        let replaced = merge(&base, &bundle, "replace").unwrap();
        assert_eq!(replaced.profiles.len(), 1);
        assert_eq!(replaced.profiles[0].settings.brightness, 71);
        assert_eq!(replaced.profiles[0].source, base.profiles[0].source);
        assert_eq!(replaced.profiles[0].shortcut, base.profiles[0].shortcut);
        assert!(replaced.profiles[0].favorite);
        assert!(merge(&base, &bundle, "skip").unwrap() == base);
        bundle.profiles[0].settings.brightness = 101;
        assert!(merge(&base, &bundle, "replace").is_err());
        let legacy = serde_json::to_string(&base).unwrap();
        let migrated = parse(&legacy).unwrap();
        assert_eq!(migrated.version, 1);
        assert!(!serde_json::to_string(&migrated)
            .unwrap()
            .contains("private-screen-path"));
    }
    #[test]
    fn bad_versions_private_fields_oversized_files_and_invalid_palettes_are_rejected() {
        let bundle = portable(&library(), &["desk".into()], &[]).unwrap();
        let mut value = serde_json::to_value(&bundle).unwrap();
        value["version"] = 2.into();
        assert!(parse(&value.to_string()).is_err());
        value["version"] = 1.into();
        value["profiles"][0]["source"] = serde_json::json!({"screenId":"private"});
        assert!(parse(&value.to_string()).is_err());
        assert!(parse(&" ".repeat(MAX_FILE_BYTES + 1)).is_err());
        assert!(validate_colors(
            &[Palette {
                id: "bad".into(),
                name: "Bad".into(),
                colors: vec!["💡".into()]
            }],
            &[]
        )
        .is_err());
        let mut base = library();
        for i in 1..40 {
            let mut p = base.profiles[0].clone();
            p.id = format!("p-{i}");
            p.shortcut.clear();
            base.profiles.push(p);
        }
        assert!(merge(&base, &bundle, "copy").is_err());
    }
}
#[tauri::command]
pub async fn import_profiles(
    app: tauri::AppHandle,
    bundle: Bundle,
    expected: Library,
    policy: String,
) -> Result<Library, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let store = app.state::<Store>();
        let mut current = store.library.lock().map_err(|e| e.to_string())?;
        if *current != expected {
            return Err(
                "Your library changed. Choose the file again to review the current conflicts."
                    .into(),
            );
        }
        if serde_json::to_vec(&bundle)
            .map_err(|e| e.to_string())?
            .len()
            > MAX_FILE_BYTES
        {
            return Err("Scene file is too large".into());
        }
        let next = merge(&current, &bundle, &policy)?;
        backup_library(&store.path, &current)?;
        replace_library(&app, &mut current, next, &store.path)?;
        Ok(current.clone())
    })
    .await
    .map_err(|e| e.to_string())?
}
fn backup_library(path: &Path, current: &Library) -> Result<(), String> {
    let backup = path.with_file_name("profiles.before-import.json");
    if !path.exists() {
        return save_json(current, &backup);
    }
    // Preserve original bytes even if startup could not decode the old file.
    let directory = path.parent().ok_or("Invalid library path")?;
    let file = tempfile::NamedTempFile::new_in(directory).map_err(|e| e.to_string())?;
    std::fs::copy(path, file.path()).map_err(|e| e.to_string())?;
    file.as_file().sync_all().map_err(|e| e.to_string())?;
    file.persist(backup).map_err(|e| e.to_string())?;
    Ok(())
}
fn write_export(directory: &Path, bundle: &Bundle) -> Result<String, String> {
    std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let mut file = tempfile::Builder::new()
        .prefix("DynamicLight-scenes-")
        .suffix(".json")
        .tempfile_in(directory)
        .map_err(|e| e.to_string())?;
    use std::io::Write;
    file.write_all(&serde_json::to_vec_pretty(bundle).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    file.as_file().sync_all().map_err(|e| e.to_string())?;
    let (_, path) = file.keep().map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}
#[tauri::command]
pub async fn export_profiles(
    app: tauri::AppHandle,
    ids: Vec<String>,
    palette_ids: Vec<String>,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let library = app
            .state::<Store>()
            .library
            .lock()
            .map_err(|e| e.to_string())?
            .clone();
        let bundle = portable(&library, &ids, &palette_ids)?;
        write_export(
            &app.path().download_dir().map_err(|e| e.to_string())?,
            &bundle,
        )
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn remember_color(app: tauri::AppHandle, color: String) -> Result<Library, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if !valid_color(&color) {
            return Err("Invalid color".into());
        }
        let store = app.state::<Store>();
        let mut current = store.library.lock().map_err(|e| e.to_string())?;
        let mut next = current.clone();
        next.recent_colors
            .retain(|c| !c.eq_ignore_ascii_case(&color));
        next.recent_colors.insert(0, color.to_lowercase());
        next.recent_colors.truncate(12);
        replace_library(&app, &mut current, next, &store.path)?;
        Ok(current.clone())
    })
    .await
    .map_err(|e| e.to_string())?
}
