# Studio authoring

Custom effects, zones, calibrated bias lighting, and appearance preferences are local. The editor preview never starts USB, screen capture, or audio. Apply edits the selected scene; Start remains a separate action when lighting is off. Cancel and Escape discard the draft. Undo restores up to 40 draft edits.

## Custom effects

Choose **Effects → Create / edit**. Combine up to four gradient, moving-wave, breathing-gradient, or color-keyframe layers. Layers blend over previous layers with their opacity; the first layer blends over black. Each layer has 2–16 ordered color stops, including positions 0 and 1000. Gradient positions run along the strip; sequence positions run through time.

Set duration (0.1–3600 seconds), repeat count (0 means forever), speed (50 means the chosen duration), and spatial direction. A finite effect holds its last frame. Applying a changed effect or pressing Start restarts its timeline; changing brightness does not. Duration governs each cycle, independently of USB frame rate. Processing is bounded by 127 LEDs, four layers, sixteen stops, and the existing 10/20/30 FPS engine.

Name and save reusable effects independently of profiles. Saving a preset does not apply it. Effects also travel inside saved scenes. Import/export uses the data-only `dynamiclight-effect` version 1 JSON wrapper; files cannot execute scripts. Import is limited to 32 KiB and native validation checks every field.

## Interactive layout and zones

Choose **Device setup → LED layout & zones**. Click LEDs around the simulated monitor, use the numbered buttons, or select an edge. Left/right arrows and Home/End move keyboard selection; Shift extends a range. The ① marker shows where the first hardware LED falls after reversal and offset. Route geometry uses an illustrative 16:9 monitor; the separate guided calibration is still the way to identify physical LEDs.

Create a named zone from the selected range, then choose a static color, the current custom effect, or a saved effect. Up to sixteen zones are supported. Later zones replace earlier overlapping zones. Unassigned LEDs follow the underlying mode. Ranges are stored as thousandths of the calibrated route and scale proportionally when LED count changes; inspect the preview after changing count. Apply saves zones with scene settings. Save a scene to reuse or share the whole layout and lighting choice.

Adjust coverage, reversal, or offset in the draft and preview before applying. Undo and Cancel restore draft state without touching hardware. Global Stop, brightness, maximum brightness, transition limit, lifecycle pauses, and capture consent still take precedence. Zones cannot turn on lighting or bypass an unavailable capture source.

## Bias-light calibration

Choose **Color → Bias-light calibration**. Set an approximate 2700–10000 K white target, brightness, and measured per-channel gains (0–200%). Record the measurement reference in the optional note. Calibration presets are stored separately from decorative effects. Applying bias lighting removes decorative zones so the entire strip uses the calibration; ordinary power controls remain in force. Choosing an ordinary color, including from the tray, exits bias calibration.

The temperature swatches are RGB approximations. They do not establish spectral accuracy, CRI, or a measured white point. Use a suitable meter and measured gains to tune your particular strip. A calibration note is a user record, not independent verification.

## Image palettes

Open the saved-palette editor and select a local PNG, JPEG, WebP, or BMP (up to 20 MiB / 40 megapixels). The app samples at most a 128×128 thumbnail and suggests up to eight prominent distinct colors; transparent pixels are ignored. Edit, reorder, name, and save the resulting palette, then apply it to Music, Aurora, Breathe, or Comet. The image is never uploaded or saved by DynamicLight.

**Use Windows wallpaper** reads the OS-selected static wallpaper only. It uses the documented [Windows wallpaper query](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-systemparametersinfow). Dynamic/video wallpaper and unavailable files require manual image selection. On macOS and Linux, choose the wallpaper image file manually. No arbitrary path-reading command is exposed to the renderer.

## Appearance

Settings offers light, dark, or system theme; violet, blue, green, or rose accents; and compact/spacious density. Preferences persist separately from lighting scenes and synchronize with Quick controls. System theme follows OS changes. Native tray/menu appearance remains governed by platform support; custom accent/density styling applies to webview surfaces.

Automated checks cover text contrast for every theme/accent pair, narrow layouts, keyboard selection, draft cancellation, effect bounds, scene serialization, and comfort precedence. Physical strip output and Windows/macOS screen-reader checks remain tracked in #37, #38, and #45.
