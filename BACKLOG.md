# DynamicLight backlog

Reviewed 2026-10-19: **3 future features**, **3 validation trackers**, **33 delivered features**. Next implementation batch: [#17](https://github.com/mskutin/DynamicLight/issues/17), [#24](https://github.com/mskutin/DynamicLight/issues/24), [#27](https://github.com/mskutin/DynamicLight/issues/27).

[GitHub project](https://github.com/users/mskutin/projects/4) · [Open issues](https://github.com/mskutin/DynamicLight/issues)

Backlog means uncommitted future work. In Progress is reserved for active implementation. Validation tracks remaining checks and fixes; Done records delivered implementation with verification handed off explicitly. CI builds do not establish physical hardware or accessibility results.

## Validation

Physical testing remains deferred. Record results in these issues; do not mark unchecked scenarios passed based on builds or simulations.

- **[#37](https://github.com/mskutin/DynamicLight/issues/37) — Validate accessibility across studio, Quick controls and tray.** Complete keyboard, contrast, scaling and Windows/macOS screen-reader validation.
- **[#38](https://github.com/mskutin/DynamicLight/issues/38) — Validate macOS preview releases on Intel and Apple Silicon.** Physical Mac USB, capture, permission, tray and lifecycle validation.
- **[#45](https://github.com/mskutin/DynamicLight/issues/45) — Validate Windows and Linux preview releases on physical hardware.** Physical Windows/Linux lifecycle, capture, USB and desktop checks; see the linked tracker.

## Future work

P2 expands product capability; P3 covers exploratory features and integrations. P1 is currently reserved for outstanding validation. Phase groups are not release dates. Dependencies below refer only to unfinished work; shipped foundations are linked in each issue.

### Ecosystem

| Issue | Feature | Priority | Depends on |
| --- | --- | --- | --- |
| [#17](https://github.com/mskutin/DynamicLight/issues/17) | Add a paired phone remote on the local network | P3 | [#16](https://github.com/mskutin/DynamicLight/issues/16) |
| [#24](https://github.com/mskutin/DynamicLight/issues/24) | Add game telemetry integrations | P3 | [#16](https://github.com/mskutin/DynamicLight/issues/16) |
| [#27](https://github.com/mskutin/DynamicLight/issues/27) | Define a plugin system for effects and integrations | P3 | — |

## Delivered

These implementation issues are closed. Their outstanding physical checks are tracked separately above.

| Issue | Delivered work | Validation |
| --- | --- | --- |
| [#1](https://github.com/mskutin/DynamicLight/issues/1) | Restore lighting automatically at startup and after reconnect | [#45](https://github.com/mskutin/DynamicLight/issues/45), [#38](https://github.com/mskutin/DynamicLight/issues/38), [#37](https://github.com/mskutin/DynamicLight/issues/37) |
| [#2](https://github.com/mskutin/DynamicLight/issues/2) | Follow display sleep, computer lock, and wake state | [#45](https://github.com/mskutin/DynamicLight/issues/45), [#38](https://github.com/mskutin/DynamicLight/issues/38), [#37](https://github.com/mskutin/DynamicLight/issues/37) |
| [#3](https://github.com/mskutin/DynamicLight/issues/3) | Improve screen sync with black-bar detection and color calibration | [#45](https://github.com/mskutin/DynamicLight/issues/45), [#38](https://github.com/mskutin/DynamicLight/issues/38), [#37](https://github.com/mskutin/DynamicLight/issues/37) |
| [#4](https://github.com/mskutin/DynamicLight/issues/4) | Add a guided LED-by-LED calibration wizard | [#45](https://github.com/mskutin/DynamicLight/issues/45), [#38](https://github.com/mskutin/DynamicLight/issues/38), [#37](https://github.com/mskutin/DynamicLight/issues/37) |
| [#5](https://github.com/mskutin/DynamicLight/issues/5) | Save profiles and switch them with global shortcuts | [#45](https://github.com/mskutin/DynamicLight/issues/45), [#38](https://github.com/mskutin/DynamicLight/issues/38), [#37](https://github.com/mskutin/DynamicLight/issues/37) |
| [#6](https://github.com/mskutin/DynamicLight/issues/6) | Add bass, frequency-band, and beat-reactive music effects | [#45](https://github.com/mskutin/DynamicLight/issues/45), [#38](https://github.com/mskutin/DynamicLight/issues/38), [#37](https://github.com/mskutin/DynamicLight/issues/37) |
| [#7](https://github.com/mskutin/DynamicLight/issues/7) | Add connection diagnostics and redacted log export | [#45](https://github.com/mskutin/DynamicLight/issues/45), [#38](https://github.com/mskutin/DynamicLight/issues/38), [#37](https://github.com/mskutin/DynamicLight/issues/37) |
| [#8](https://github.com/mskutin/DynamicLight/issues/8) | Support multiple monitors and independent LED controllers | [#45](https://github.com/mskutin/DynamicLight/issues/45), [#38](https://github.com/mskutin/DynamicLight/issues/38), [#37](https://github.com/mskutin/DynamicLight/issues/37) |
| [#9](https://github.com/mskutin/DynamicLight/issues/9) | Switch profiles automatically for selected applications | [#45](https://github.com/mskutin/DynamicLight/issues/45), [#38](https://github.com/mskutin/DynamicLight/issues/38), [#37](https://github.com/mskutin/DynamicLight/issues/37) |
| [#10](https://github.com/mskutin/DynamicLight/issues/10) | Schedule lighting scenes and gradual transitions | [#45](https://github.com/mskutin/DynamicLight/issues/45), [#38](https://github.com/mskutin/DynamicLight/issues/38), [#37](https://github.com/mskutin/DynamicLight/issues/37) |
| [#11](https://github.com/mskutin/DynamicLight/issues/11) | Build a custom lighting effects editor | [#45](https://github.com/mskutin/DynamicLight/issues/45), [#38](https://github.com/mskutin/DynamicLight/issues/38), [#37](https://github.com/mskutin/DynamicLight/issues/37) |
| [#12](https://github.com/mskutin/DynamicLight/issues/12) | Control independent strip zones | [#45](https://github.com/mskutin/DynamicLight/issues/45), [#38](https://github.com/mskutin/DynamicLight/issues/38), [#37](https://github.com/mskutin/DynamicLight/issues/37) |
| [#13](https://github.com/mskutin/DynamicLight/issues/13) | Capture a selected window or screen region | [#45](https://github.com/mskutin/DynamicLight/issues/45), [#38](https://github.com/mskutin/DynamicLight/issues/38), [#37](https://github.com/mskutin/DynamicLight/issues/37) |
| [#14](https://github.com/mskutin/DynamicLight/issues/14) | Add brightness and flash comfort controls | [#45](https://github.com/mskutin/DynamicLight/issues/45), [#38](https://github.com/mskutin/DynamicLight/issues/38), [#37](https://github.com/mskutin/DynamicLight/issues/37) |
| [#15](https://github.com/mskutin/DynamicLight/issues/15) | Show temporary notification accents | [#45](https://github.com/mskutin/DynamicLight/issues/45), [#38](https://github.com/mskutin/DynamicLight/issues/38), [#37](https://github.com/mskutin/DynamicLight/issues/37) |
| [#16](https://github.com/mskutin/DynamicLight/issues/16) | Expose a local automation API and command-line controls | [#45](https://github.com/mskutin/DynamicLight/issues/45), [#38](https://github.com/mskutin/DynamicLight/issues/38), [#37](https://github.com/mskutin/DynamicLight/issues/37) |
| [#18](https://github.com/mskutin/DynamicLight/issues/18) | Export and import portable profiles across platforms | [#45](https://github.com/mskutin/DynamicLight/issues/45), [#38](https://github.com/mskutin/DynamicLight/issues/38), [#37](https://github.com/mskutin/DynamicLight/issues/37) |
| [#19](https://github.com/mskutin/DynamicLight/issues/19) | Add a neutral-white bias-lighting mode | [#45](https://github.com/mskutin/DynamicLight/issues/45), [#38](https://github.com/mskutin/DynamicLight/issues/38), [#37](https://github.com/mskutin/DynamicLight/issues/37) |
| [#20](https://github.com/mskutin/DynamicLight/issues/20) | Blend screen colors with music-reactive brightness | [#45](https://github.com/mskutin/DynamicLight/issues/45), [#38](https://github.com/mskutin/DynamicLight/issues/38), [#37](https://github.com/mskutin/DynamicLight/issues/37) |
| [#21](https://github.com/mskutin/DynamicLight/issues/21) | Extract lighting palettes from images and wallpaper | [#45](https://github.com/mskutin/DynamicLight/issues/45), [#38](https://github.com/mskutin/DynamicLight/issues/38), [#37](https://github.com/mskutin/DynamicLight/issues/37) |
| [#22](https://github.com/mskutin/DynamicLight/issues/22) | Record and replay lighting sequences | [#45](https://github.com/mskutin/DynamicLight/issues/45), [#38](https://github.com/mskutin/DynamicLight/issues/38), [#37](https://github.com/mskutin/DynamicLight/issues/37) |
| [#23](https://github.com/mskutin/DynamicLight/issues/23) | Adapt brightness to an ambient-light sensor | [#45](https://github.com/mskutin/DynamicLight/issues/45), [#38](https://github.com/mskutin/DynamicLight/issues/38), [#37](https://github.com/mskutin/DynamicLight/issues/37) |
| [#25](https://github.com/mskutin/DynamicLight/issues/25) | Add focus and break timers with lighting cues | [#45](https://github.com/mskutin/DynamicLight/issues/45), [#38](https://github.com/mskutin/DynamicLight/issues/38), [#37](https://github.com/mskutin/DynamicLight/issues/37) |
| [#26](https://github.com/mskutin/DynamicLight/issues/26) | Coordinate device handoff between Windows and Mac | [#45](https://github.com/mskutin/DynamicLight/issues/45), [#38](https://github.com/mskutin/DynamicLight/issues/38), [#37](https://github.com/mskutin/DynamicLight/issues/37) |
| [#28](https://github.com/mskutin/DynamicLight/issues/28) | Make the monitor and LED preview interactive | [#45](https://github.com/mskutin/DynamicLight/issues/45), [#38](https://github.com/mskutin/DynamicLight/issues/38), [#37](https://github.com/mskutin/DynamicLight/issues/37) |
| [#29](https://github.com/mskutin/DynamicLight/issues/29) | Add a color wheel, temperature control, and saved palettes | [#45](https://github.com/mskutin/DynamicLight/issues/45), [#38](https://github.com/mskutin/DynamicLight/issues/38), [#37](https://github.com/mskutin/DynamicLight/issues/37) |
| [#30](https://github.com/mskutin/DynamicLight/issues/30) | Create a visual scene-card library | [#45](https://github.com/mskutin/DynamicLight/issues/45), [#38](https://github.com/mskutin/DynamicLight/issues/38), [#37](https://github.com/mskutin/DynamicLight/issues/37) |
| [#31](https://github.com/mskutin/DynamicLight/issues/31) | Build focused mode-specific control panels | [#45](https://github.com/mskutin/DynamicLight/issues/45), [#38](https://github.com/mskutin/DynamicLight/issues/38), [#37](https://github.com/mskutin/DynamicLight/issues/37) |
| [#32](https://github.com/mskutin/DynamicLight/issues/32) | Add a compact floating lighting controller | [#45](https://github.com/mskutin/DynamicLight/issues/45), [#38](https://github.com/mskutin/DynamicLight/issues/38), [#37](https://github.com/mskutin/DynamicLight/issues/37) |
| [#33](https://github.com/mskutin/DynamicLight/issues/33) | Add a rich tray and macOS menu-bar control panel | [#45](https://github.com/mskutin/DynamicLight/issues/45), [#38](https://github.com/mskutin/DynamicLight/issues/38), [#37](https://github.com/mskutin/DynamicLight/issues/37) |
| [#34](https://github.com/mskutin/DynamicLight/issues/34) | Add drag-and-drop monitor and strip layout setup | [#45](https://github.com/mskutin/DynamicLight/issues/45), [#38](https://github.com/mskutin/DynamicLight/issues/38), [#37](https://github.com/mskutin/DynamicLight/issues/37) |
| [#35](https://github.com/mskutin/DynamicLight/issues/35) | Show actionable connection and capture states | [#45](https://github.com/mskutin/DynamicLight/issues/45), [#38](https://github.com/mskutin/DynamicLight/issues/38), [#37](https://github.com/mskutin/DynamicLight/issues/37) |
| [#36](https://github.com/mskutin/DynamicLight/issues/36) | Add themes, accent colors, and layout density options | [#45](https://github.com/mskutin/DynamicLight/issues/45), [#38](https://github.com/mskutin/DynamicLight/issues/38), [#37](https://github.com/mskutin/DynamicLight/issues/37) |

Sources: [scope and dependencies](docs/backlog.json), [delivery and validation notes](docs/backlog-progress.json), [GitHub mapping](docs/github-backlog.json). Regenerate with `node scripts/render-backlog.cjs`. GitHub is authoritative for live status; update these snapshots when reconciling the roadmap.
