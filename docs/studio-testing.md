# Studio validation

For features added through 0.16, also run the [backlog validation handoff](backlog-validation.md). It covers compositions/zones, capture/replay, routines, multiple controllers, sensors, games/plugins and the phone remote. All physical results remain pending.

Physical checks are deferred at the user's request. Automated tests and successful packages are not evidence that a particular strip, desktop permission flow, or audio route works.

Record results and defects in [Windows/Linux validation #45](https://github.com/mskutin/DynamicLight/issues/45), [macOS validation #38](https://github.com/mskutin/DynamicLight/issues/38), and [accessibility #37](https://github.com/mskutin/DynamicLight/issues/37). These trackers own remaining verification after implementation issues close. Include the tested build and environment; leave untested checks pending.

After the settings refactor, also check interleaved Studio/tray/Quick-controls edits: changing color in Studio must preserve a newer Quick-controls brightness change, and idle saves must not undo the latest edit to the same field. Load existing profiles, update their display selection, restart, and confirm scene settings and local bindings survive while calibration, comfort limits and capture-resume preferences remain local.

For scene sharing and color tools, export a selection on one platform and import it on another. Check copy/replace/skip conflicts, the pre-import backup, capacity errors and an intervening library edit. Verify imports never start capture or USB. Exercise color-wheel pointer controls, keyboard sliders, RGB/hex, Apply/Reset/Cancel, approximate temperature, palette color order/replacement/deletion, recent colors and persistence. Compare palette output on physical strips; the temperature control is not measured white calibration. In Quick controls, verify event-driven favorites/pin initialization and recovery from a transient failed brightness preview.

## Automated coverage

- Rust: profile persistence/validation, calibration apply/cancel/Stop and lock transitions using an isolated USB transport, symmetric black-bar detection with hysteresis, color adjustments, hard brightness cap, startup rate limiting, spectral-flux onsets, sustained tones/silence/refractory period, and blend behavior.
- Browser: profile creation/duplication/application, calibration apply/Escape cancellation, all mode controls, gain/blend settings, external settings synchronization, responsive widths, CSP, and existing recovery/preview coverage.
- Windows native smoke (`scripts/smoke.cjs`, isolated app identifier): profile save/read, actual global shortcut registration/unregistration, duplicate binding rejection without changing the saved collection, real IPC and effect previews. No physical USB or capture unless `--hardware` is supplied.
- CI: Windows, both Mac architectures, Linux packages; Linux virtual audio and Xvfb capture.

## Physical checklist

1. Create Work and Movies profiles. Rename, duplicate, reorder, favorite, update settings, then restart and verify persistence. Select from the tray and shortcuts. Check OS shortcut conflicts leave the old binding working. Verify selecting a capture profile requests permission only when needed.
2. With the window hidden, change brightness and Stop from the tray. Reopen and verify matching state. Check tray appearance and keyboard navigation on each desktop.
3. Calibrate from both running and stopped states. Step first/last LEDs; vary count, offset, direction and coverage. Verify mapped red/green/blue/amber edges before applying. Cancel, Escape, Stop, lock, sleep and disconnect during a test. Stop must never restore an earlier running scene.
4. Play letterboxed, pillarboxed, full-frame, dark and rapidly changing SDR video. Enable/disable bar exclusion, adjust/reset RGB, saturation and gamma. Verify crop settling, transitions, response at 10/20/30 FPS, and brightness limits. Record HDR/protected-content limitations.
5. Play steady tones, silence, speech and percussion through system audio and microphone. Verify gain and gate behavior, beat accents, and absence of repeated beats on sustained tones. Beat pulse detects onsets; it does not estimate BPM or promise musical beat tracking.
6. In Blend, test silence and both source failures independently. Screen failure falls back to the chosen music style; audio failure uses screen colors. Retry restores both. When automatic capture recovery is enabled, only the missing source is retried without interrupting the remaining source. Stop and lifecycle permissions still apply. Existing smoothing and all brightness limits apply after blending.

## Design limits

Black-bar detection samples a bounded grid, accepts symmetric borders up to one quarter of each dimension, waits 600 ms for a stable change and retains its crop on entirely dark frames. It is optional and can mistake sustained, symmetric dark picture content for borders. It does not capture protected video or perform HDR tone mapping.

Profiles store lighting settings locally, with a maximum of 40 profiles. They preserve current hardware mapping, comfort limits and startup/privacy settings when applied. Shortcuts register only when configured; a conflicting registration prevents the edit from being saved. Removing a binding before assigning it to another profile supports deliberate reassignment. Recent tray colors are kept for the current app session. Quick controls (#32) now shares the same worker, with optional pinning and saved placement.

Calibration test patterns use at most 20% brightness and respect a lower user cap. Individual LED tests use physical order; the edge test uses calibrated order. Calibration is never persisted as the active scene.

All modes use the same Rust lighting worker and USB writer. Screen/audio data remain local. Native platform hardware and permission validation is still pending.

## Diagnostics and mode memory

Rust tests cover mode persistence through restart, separation from hardware/comfort settings and capture consent, invalid-mode rejection, direction mirroring, missed-deadline accounting, bounded history and allowlisted output. Native smoke checks the commands without starting hardware. History is opt-in, flushed at most every five seconds, bounded to 200 state transitions and seven days. Abrupt exit can lose the last five seconds. Counters cover this app session; missed slots exclude deliberate idle frames and sleep, and do not measure capture-source frame drops.

Physical follow-up: tune each mode, switch and restart; verify remembered source choices still honor OS permission. Exercise a failed connection/capture and verify report guidance. Enable history, restart, clear it and disable it. Check copy and Downloads export on each OS.

## Quick controls and scenes (0.6)

Automated checks cover native compact-window creation, real brightness synchronization back to the main window, pin/unpin, and safe disconnected operation. Rust tests exercise off-screen position clamping, starter validity, bounded preview requests and native effect parity. Browser checks cover scene search, filters, Save a copy, simulated previews and reduced motion, plus compact brightness, Stop, recovery status and responsive widths.

Physical follow-up: move Quick controls between differently scaled monitors; close/reopen, restart, then disconnect a monitor and reopen. Verify keyboard use, pinning and return to studio on Windows/macOS/Linux. Window placement and always-on-top depend on the desktop compositor; Wayland may control placement itself. Apply favorites from all three surfaces, and test Stop during capture failure. Test all four starters and saved scenes on the strip.

Scene cards always show examples, never live captures. Screen examples use synthetic colors and music examples use synthetic input; effect examples use the same native animation functions as output. The current-settings marker compares relevant controls. This is not a claim that a particular saved profile is actively controlling physical LEDs.
