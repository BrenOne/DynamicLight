# Developer guide

## Develop

Install [Tauri's system prerequisites](https://v2.tauri.app/start/prerequisites/), current stable Rust (tested with 1.99.1), Node.js 24, and pnpm 11. Windows needs the Visual Studio C++ build tools and Windows SDK; macOS needs Xcode command-line tools. For Linux, use Ubuntu 24.04+ and the native packages listed in `.github/workflows/build.yml`, including WebKitGTK 4.1, ALSA, udev, XCB, PipeWire, and libclang development headers. `pulseaudio-utils` provides the system-audio monitor helper. See [Linux setup](linux.md) for runtime permissions.

```sh
pnpm install --frozen-lockfile
pnpm start                 # Native app with development rebuilds
pnpm test                  # Rust protocol, settings, audio, sampling, lifecycle tests
pnpm check                 # Clippy, with warnings treated as errors
pnpm exec playwright install chromium
pnpm test:ui               # Browser UI tests with isolated IPC fixtures
pnpm run build:win         # Windows NSIS installer, on Windows
pnpm run build:mac         # macOS DMG for the host architecture, on macOS
pnpm run build:linux       # Debian package and AppImage, on Linux
```

For another Mac architecture, use `rustup target add x86_64-apple-darwin` or `aarch64-apple-darwin`, then `pnpm tauri build --target <target> --bundles dmg`. CI builds Windows x64, Linux x64, and both Mac architectures. Bundles appear in `src-tauri/target[/<target>]/release/bundle/`. Cargo.lock and pnpm-lock.yaml are committed. Build on the minimum Linux distribution you intend to support; newer glibc binaries cannot run on older distributions.

`scripts/smoke.cjs` tests the packaged Windows UI and IPC through WebView2's local debugging port using the Playwright development dependency. Close any running DynamicLight first, or build a separate test identifier with Tauri's `--config` option to isolate its settings and single-instance lock. `node scripts/smoke.cjs --hardware` additionally changes the strip and briefly captures screen/system audio. Ordinary tests never access USB or start capture. Set `DYNAMICLIGHT_EXECUTABLE` to test an installed executable. The debugging port is enabled only in that test process.

`pnpm test:ui` runs the real UI against explicit browser-only IPC fixtures, checking six responsive widths, effect/music controls, live metrics, error recovery, and slider coalescing. It does not test physical hardware. Set `UI_BROWSER_CHANNEL=msedge` to use an installed Edge browser, or `PLAYWRIGHT_PATH` to reuse an existing Playwright installation. Screenshots are written to the ignored `.local/` directory.

## Implementation

Linux CI also exercises computer-audio input against a PulseAudio null sink, X11 capture under Xvfb, validates the udev rule, inspects the Debian package's icon/desktop entry/dependencies, and launches the packaged app on a virtual desktop. These are explicit integration tests; normal unit tests do not capture audio or a screen. Physical Linux USB, sleep, real desktop audio routing, and Wayland compositor behavior still need validation.

`src-tauri/src/linux.rs` owns the `parec` monitor subprocess (48 kHz mono float input) and systemd-logind sleep signals. Stopping capture kills and reaps the audio child before joining its reader. The sleep watcher uses a delay inhibitor when available and acknowledges the lighting worker before releasing it. Non-systemd desktops retain manual operation. X11 capture uses xcap in memory; Wayland still-image fallbacks are deliberately disabled because they may prompt repeatedly or write screenshots to disk. A future Wayland implementation should use a persistent ScreenCast portal/PipeWire stream.

- `src-tauri/src/protocol.rs` implements RB commands and SC frames, split into 64-byte payloads plus report ID 0.
- `device.rs` opens only the lighting interface, rejects the keyboard interface, checks complete writes, and drains replies. A single worker owns all USB operations.
- `engine.rs` owns settings, capture lifetimes, effects, smoothing, reconnect, and frame pacing. It does not depend on JavaScript timers. Preview events are limited to 10 Hz and suppressed while the window is unfocused, hidden, or minimized.
- `effects.rs` generates five time-based effects and four music styles. Music uses 32 logarithmic FFT bands, an RMS noise gate, fast attack, and a frame-rate-independent release envelope. The bass style responds to low-frequency energy; it does not estimate BPM. Brightness and color smoothing are applied after effect generation.
- `previewSettings` validates and applies controls without disk writes. The UI keeps one preview request in flight and coalesces subsequent edits, then commits atomic settings after 650 ms of inactivity. A native `Start` action owns connect, power, and capture intent. Changing an active source or display explicitly restarts capture; palette/style changes keep the audio stream running. Startup restore and automatic capture resume are separate opt-ins.
- Telemetry measures completed frame/solid refresh writes, latest write duration, failures, and age of the last successful refresh. Initialization and explicit blackout commands are excluded from these counters. Colors represent generated output; HID replies are drained but not decoded as physical LED feedback. Audio RMS is the input signal before gating; spectrum bars include gain and release.
- `capture.rs` enumerates displays through xcap. Windows uses Desktop Duplication with direct edge sampling from a reusable GPU staging texture, with a small GDI thumbnail as a fallback for unsupported or rotated displays. macOS uses native xcap capture. `audio.rs` uses bounded sample buffers and RustFFT. The macOS bridge handles ScreenCaptureKit system audio, microphone permission, and sleep notifications.
- The HTML/CSS/JavaScript UI sends validated commands through Tauri IPC. It has no Node.js, filesystem, shell, or remote network access. Runtime assets are embedded in the executable.

Settings are atomically saved under the OS application-data directory in `com.dynamiclight.desktop/settings.json`. On first launch, valid settings from the Electron prototype's `dynamiclight/settings.json` are imported. Legacy display IDs reset to the primary display because native display IDs differ. The old file is retained.

Interactive settings changes use `patchSettings` with only the edited fields. The native worker validates and merges each patch into its current settings: unrelated fields compose, and the last processed edit wins for the same field. Preview edits do not write to disk; an empty persistent patch saves current native state, so a delayed save cannot replay an old window snapshot. Studio keeps only unacknowledged edits over incoming state. Quick controls and tray adjustments use the same native path. Whole-settings commands remain for explicit replacement and compatibility tests, not routine UI edits.

Profiles store portable `SceneSettings` separately from the local `source.screenId` binding. Layout, lifecycle preferences, capture-resume consent, comfort limits and mode memory stay in installation settings. Existing profiles containing full settings load through a compatibility decoder; the next library save writes the separated representation. This is groundwork for import/export, not a public interchange format. Older app versions do not understand the new `source` field. Profile application still selects the saved local display and follows normal capture permission rules.

`engine.rs` retains one worker and one USB owner. Its settings transaction, calibration session, capture reconciliation and frame generation have separate methods within the module; the frame loop coordinates them in order. Extend these boundaries rather than adding another writer or bypassing lifecycle state.

`run-state.json` separately persists requested on/off state on explicit Start/Stop/Disconnect, not transient sleep or USB failure. Quitting preserves that intent. `lifecycle.rs` combines independent sleep, lock, and display flags; clearing one cannot resume while another still applies. The native worker retries USB/capture every three seconds, polls the chosen monitor or default CPAL device every two seconds while capturing, and never substitutes another explicitly selected monitor. OS events use a separate channel so a full UI command queue cannot drop suspend notifications. On suspend the worker blacks out (if enabled) and closes USB before stream teardown. Automatic Mac capture preflights permission without prompting; a deliberate retry can request permission.

Windows uses suspend callbacks plus a hidden native window for display-power, session-lock, and topology notifications. Mac uses NSWorkspace notifications and best-effort distributed screen-lock notifications (not a documented public lock API), with initial session/display queries. Linux uses logind `PrepareForSleep` / `LockedHint`, X11 DPMS, and `pactl subscribe` for default-output changes. Unsupported monitoring is reported in the UI; Wayland display-only sleep remains unsupported. Physical lifecycle validation is tracked in [the test checklist](lifecycle-testing.md).

Login startup uses the [Tauri autostart plugin](https://v2.tauri.app/plugin/autostart/) through a narrow native command. The checkbox reads the OS registration rather than a duplicated settings flag, and failed changes restore the previous UI value. No login entry is created on installation or first launch. `--autostart` keeps Windows/Mac in the tray and Linux minimized.

Protocol references: [HdrBridge wire format](https://github.com/GetTheNya/HdrBridge/blob/main/node_strip_test.js), [SyncLight RB commands](https://github.com/jakebuild/synclight), and [section command investigation](https://github.com/tanmaster/light-control).

Effect thumbnails call the read-only `effect_previews` command at up to 10 Hz. It evaluates the same Rust effect functions at 48 sample positions with the selected palette, speed, and brightness, without USB or capture. Requests stop offscreen, on blur, when hidden, or outside Effects mode. Reduced motion uses a fixed phase. `src/ui/icon.svg` is the shared UI/native icon source; regenerate native assets with `pnpm tauri icon src/ui/icon.svg`, retaining the PNG, ICO and ICNS bundle assets.
