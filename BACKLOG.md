# DynamicLight backlog

Reviewed 2026-10-19 against merged PRs #39 and #40. Partial means some acceptance criteria are implemented; it does not mean the issue is complete. Platform builds do not replace physical validation.

Priority: **P1** = foundations and core usability; **P2** = the next layer of product capability; **P3** = exploratory or integration work. Dependencies are recorded in the issue bodies.

[GitHub project](https://github.com/users/mskutin/projects/4) · [All issues](https://github.com/mskutin/DynamicLight/issues)

## Agreed order

1. Startup, sleep, lock and reconnect (#1, #2), with actionable recovery states.
2. Saved profiles and tray controls (#5, #33).
3. Guided strip calibration and screen sampling (#4, #3).
4. Finish beat detection, then screen/music blending (#6, #20).

Custom editors, plugins, phone control and game integrations remain deferred. Physical Mac/Linux validation remains a release requirement.

## Implemented portions and remaining work

- **[REL-01 · #1](https://github.com/mskutin/DynamicLight/issues/1) — In progress.** USB retry and single-instance protection are in main. Startup restore and login controls are being added in codex/startup-and-recovery. Remaining: Review the lifecycle PR and validate startup, replug, busy-device recovery and capture permissions on physical hardware. Saved profiles are a separate follow-up (#5).
- **[REL-02 · #2](https://github.com/mskutin/DynamicLight/issues/2) — In progress.** System-sleep blackout is in main. Independent display/lock handling and intended-state restoration are being added in codex/startup-and-recovery. Remaining: Review and physically validate overlapping lock/display/suspend transitions. Wayland display-only detection remains unsupported; Mac lock events are best effort.
- **[AUD-01 · #6](https://github.com/mskutin/DynamicLight/issues/6) — Partial.** Spectrum, bass pulse, level meter, color wave, sensitivity, noise gate, release, system/microphone selection and live meters shipped in #40. Remaining: True beat/onset detection, explicit gain control, and real-device permission/routing validation across platforms.
- **[REL-03 · #7](https://github.com/mskutin/DynamicLight/issues/7) — Partial.** USB frame rate, write counts/failures/latency, capture resolution and audio telemetry shipped in #40. Remaining: Dropped-frame accounting, bounded local logs, redacted export and a copyable recovery summary.
- **[UI-04 · #31](https://github.com/mskutin/DynamicLight/issues/31) — Partial.** Focused mode panels, live audio spectrum, effect speed and source selection shipped in #40. Remaining: Effect direction, screen calibration, and independent saved settings for each mode.
- **[UI-08 · #35](https://github.com/mskutin/DynamicLight/issues/35) — Partial.** Separate connection/capture/output feedback and single-action startup shipped in #40. Explicit paused/retrying states are part of the lifecycle PR. Remaining: Review lifecycle states, then share the same model with future tray and compact controllers and validate assistive announcements.
- **[UI-10 · #37](https://github.com/mskutin/DynamicLight/issues/37) — Partial.** Semantic controls, visible focus, responsive layouts and reduced-motion effect previews shipped in #40. Remaining: Complete keyboard, contrast, scaling and Windows/macOS screen-reader validation.
- **[PLAT-01 · #38](https://github.com/mskutin/DynamicLight/issues/38) — Partial.** Intel and Apple Silicon DMG builds pass CI. README documents unsigned installation and platform limitations. Remaining: Physical Mac USB, capture, permission, tray and lifecycle validation.

## Foundation

| ID | Feature | Area | Priority | Issue |
| --- | --- | --- | --- | --- |
| REL-01 | Restore lighting automatically at startup and after reconnect | reliability | P1 | [#1](https://github.com/mskutin/DynamicLight/issues/1) |
| REL-02 | Follow display sleep, computer lock, and wake state | reliability | P1 | [#2](https://github.com/mskutin/DynamicLight/issues/2) |
| SCR-01 | Improve screen sync with black-bar detection and color calibration | screen | P1 | [#3](https://github.com/mskutin/DynamicLight/issues/3) |
| HW-01 | Add a guided LED-by-LED calibration wizard | hardware | P1 | [#4](https://github.com/mskutin/DynamicLight/issues/4) |
| REL-03 | Add connection diagnostics and redacted log export | reliability | P1 | [#7](https://github.com/mskutin/DynamicLight/issues/7) |
| FX-03 | Add brightness and flash comfort controls | effects | P1 | [#14](https://github.com/mskutin/DynamicLight/issues/14) |
| UI-08 | Show actionable connection and capture states | ui | P1 | [#35](https://github.com/mskutin/DynamicLight/issues/35) |
| UI-10 | Improve accessibility and reduced-motion support | ui | P1 | [#37](https://github.com/mskutin/DynamicLight/issues/37) |
| PLAT-01 | Build and validate macOS releases on Intel and Apple Silicon | platform | P1 | [#38](https://github.com/mskutin/DynamicLight/issues/38) |

## Studio

| ID | Feature | Area | Priority | Issue |
| --- | --- | --- | --- | --- |
| AUT-01 | Save profiles and switch them with global shortcuts | automation | P1 | [#5](https://github.com/mskutin/DynamicLight/issues/5) |
| AUD-01 | Add bass, frequency-band, and beat-reactive music effects | audio | P2 | [#6](https://github.com/mskutin/DynamicLight/issues/6) |
| AUT-03 | Schedule lighting scenes and gradual transitions | automation | P2 | [#10](https://github.com/mskutin/DynamicLight/issues/10) |
| FX-01 | Build a custom lighting effects editor | effects | P2 | [#11](https://github.com/mskutin/DynamicLight/issues/11) |
| FX-02 | Control independent strip zones | effects | P2 | [#12](https://github.com/mskutin/DynamicLight/issues/12) |
| SCR-02 | Capture a selected window or screen region | screen | P2 | [#13](https://github.com/mskutin/DynamicLight/issues/13) |
| AUT-05 | Export and import portable profiles across platforms | automation | P2 | [#18](https://github.com/mskutin/DynamicLight/issues/18) |
| FX-04 | Add a neutral-white bias-lighting mode | effects | P2 | [#19](https://github.com/mskutin/DynamicLight/issues/19) |
| FX-05 | Blend screen colors with music-reactive brightness | effects | P2 | [#20](https://github.com/mskutin/DynamicLight/issues/20) |
| FX-06 | Extract lighting palettes from images and wallpaper | effects | P2 | [#21](https://github.com/mskutin/DynamicLight/issues/21) |
| FX-07 | Record and replay lighting sequences | effects | P3 | [#22](https://github.com/mskutin/DynamicLight/issues/22) |
| AUT-06 | Add focus and break timers with lighting cues | automation | P3 | [#25](https://github.com/mskutin/DynamicLight/issues/25) |
| UI-01 | Make the monitor and LED preview interactive | ui | P1 | [#28](https://github.com/mskutin/DynamicLight/issues/28) |
| UI-02 | Add a color wheel, temperature control, and saved palettes | ui | P2 | [#29](https://github.com/mskutin/DynamicLight/issues/29) |
| UI-03 | Create a visual scene-card library | ui | P1 | [#30](https://github.com/mskutin/DynamicLight/issues/30) |
| UI-04 | Build focused mode-specific control panels | ui | P1 | [#31](https://github.com/mskutin/DynamicLight/issues/31) |
| UI-05 | Add a compact floating lighting controller | ui | P2 | [#32](https://github.com/mskutin/DynamicLight/issues/32) |
| UI-06 | Add a rich tray and macOS menu-bar control panel | ui | P1 | [#33](https://github.com/mskutin/DynamicLight/issues/33) |
| UI-07 | Add drag-and-drop monitor and strip layout setup | ui | P2 | [#34](https://github.com/mskutin/DynamicLight/issues/34) |
| UI-09 | Add themes, accent colors, and layout density options | ui | P2 | [#36](https://github.com/mskutin/DynamicLight/issues/36) |

## Ecosystem

| ID | Feature | Area | Priority | Issue |
| --- | --- | --- | --- | --- |
| HW-02 | Support multiple monitors and independent LED controllers | hardware | P2 | [#8](https://github.com/mskutin/DynamicLight/issues/8) |
| AUT-02 | Switch profiles automatically for selected applications | automation | P2 | [#9](https://github.com/mskutin/DynamicLight/issues/9) |
| AUT-04 | Show temporary notification accents | automation | P3 | [#15](https://github.com/mskutin/DynamicLight/issues/15) |
| INT-01 | Expose a local automation API and command-line controls | integrations | P2 | [#16](https://github.com/mskutin/DynamicLight/issues/16) |
| INT-02 | Add a paired phone remote on the local network | integrations | P3 | [#17](https://github.com/mskutin/DynamicLight/issues/17) |
| HW-03 | Adapt brightness to an ambient-light sensor | hardware | P3 | [#23](https://github.com/mskutin/DynamicLight/issues/23) |
| INT-03 | Add game telemetry integrations | integrations | P3 | [#24](https://github.com/mskutin/DynamicLight/issues/24) |
| REL-04 | Coordinate device handoff between Windows and Mac | reliability | P3 | [#26](https://github.com/mskutin/DynamicLight/issues/26) |
| INT-04 | Define a plugin system for effects and integrations | integrations | P3 | [#27](https://github.com/mskutin/DynamicLight/issues/27) |

The original planning baseline is [docs/backlog.json](docs/backlog.json); current implementation notes and sequencing are in [docs/backlog-progress.json](docs/backlog-progress.json). Each issue retains its acceptance checklist. Do not close partial issues until the remaining work and physical checks are complete.
