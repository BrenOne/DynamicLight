# Карта кода DynamicLight

Читать по нужде, а не целиком. Ищи `git grep -n`, читай диапазоны строк.

## Rust — `src-tauri/src` (~15,6k строк)
**Лента и протокол**
- `protocol.rs` — пакеты ROBOBLOQ: `frame()` (0x80), `merge()` до 34 диапазонов, команды.
- `device.rs` — HID: открытие, запись по 65 байт с паузой, повтор/переоткрытие, `query_info()` (0x82), `hold_output`, `effective_fps`/`fps_limit`.
- `controllers.rs` — несколько лент, у каждой свой движок-писатель.

**Движок** — `engine.rs` (3,1k; код до ~1830, дальше тесты)
- `Action` (команды из UI/трея/системы), `Snapshot`, `Handle::request`, `Engine::new`.
- `connect_strip` (подключение + автонастройка), `apply_settings`, `action`, калибровка `begin/step/end_calibration`.
- Захват: `begin_capture`, `interrupt_capture`, `reconcile_capture`; кадр — `generate_frame`, цикл — `tick`, запуск — `start`.
- Рутины: `sync_routines`, `apply_routine`, `tick_routines`.

**Захват и цвет**
- `capture.rs` — общий захват, восстановление DXGI, `display_aspect`.
- `capture_windows.rs` — DXGI duplication, GPU-даунскейл через mips.
- `capture_source.rs` — выбор окна/монитора.
- `lighting.rs` — раскладка: `layout()`, `auto_sides()`, `guess_sides`, `lift_dark`.
- `ambient.rs` (датчик освещённости), `audio.rs` + `beat.rs` (музыка), `effects.rs` (эффекты), `scenes.rs`, `studio.rs` (зоны/сцены), `sequence.rs` (запись RGB).

**Настройки и данные**: `settings.rs` (поля, валидация, FPS), `profiles.rs`, `sharing.rs`, `handoff.rs`, `appearance.rs`.

**Система (Windows)**
- `lifecycle.rs` / `lifecycle_windows.rs` — сон/выход/блокировка, WM_ENDSESSION → Shutdown, хук lock-клавиш.
- `autostart_windows.rs` — задачи Планировщика (COM), `--setup-tasks`, `--relaunch-after`, опция `admin_rights` (`admin_mode`, `enable_admin`, `can_elevate`).
- `tray.rs`, `compact.rs` (Quick controls), `platform.rs`, `linux.rs`.
- `updates.rs` — апдейтер (доверяет только BrenOne releases).

**Прочее (редко трогаем)**: `automation.rs`, `remote.rs`, `bounded_http.rs`, `game.rs` (CS2), `plugins.rs`, `extensions.rs`, `routines.rs`, `diagnostics.rs`.

`lib.rs` — модули и Tauri-команды (`#[tauri::command]`), связь UI ↔ движок.

## UI — `src/ui`
- `index.html` + `renderer.js` — главное окно, настройки (Screen, FPS, стороны LED).
- `ui-common.js` — общие помощники, `window.ledLayout` (превью ленты).
- `studio.js`, `compact.html/js`, `updates.js`, прочие — по имени.

## Остальное
- `control/` — CLI (в сборке форка не используется).
- `src-tauri/windows/installer-hooks.nsh` — вопросы NSIS-установщика.
- `.github/workflows/build.yml` — CI (checks + build + release по тегу).
- `scripts/release.cjs` — проверка версий.
