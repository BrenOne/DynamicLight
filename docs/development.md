# Developer guide

## Develop

Install [Tauri's system prerequisites](https://v2.tauri.app/start/prerequisites/), current stable Rust (tested with 1.99.1), Node.js 24, and pnpm 11. Windows needs the Visual Studio C++ build tools and Windows SDK; macOS needs Xcode command-line tools. For Linux, use Ubuntu 24.04+ and the native packages listed in `.github/workflows/build.yml`, including WebKitGTK 4.1, ALSA, udev, XCB, PipeWire, and libclang development headers. `pulseaudio-utils` provides the system-audio monitor helper. See [Linux setup](linux.md) for runtime permissions.

```sh
pnpm install --frozen-lockfile
pnpm start                 # Native app with development rebuilds
pnpm test                  # Rust protocol, settings, audio, sampling, lifecycle tests
pnpm check                 # Clippy, with warnings treated as errors
pnpm run build:win         # Windows NSIS installer, on Windows
pnpm run build:mac         # macOS DMG for the host architecture, on macOS
pnpm run build:linux       # Debian package and AppImage, on Linux
```

For another Mac architecture, use `rustup target add x86_64-apple-darwin` or `aarch64-apple-darwin`, then `pnpm tauri build --target <target> --bundles dmg`. CI builds Windows x64, Linux x64, and both Mac architectures. Bundles appear in `src-tauri/target[/<target>]/release/bundle/`. Cargo.lock and pnpm-lock.yaml are committed. Build on the minimum Linux distribution you intend to support; newer glibc binaries cannot run on older distributions.

`scripts/smoke.cjs` tests the packaged Windows UI and IPC through WebView2's local debugging port. Install `playwright-core` separately or set `PLAYWRIGHT_PATH` to an existing Playwright installation. Close any running DynamicLight first. `node scripts/smoke.cjs --hardware` additionally changes the strip and briefly captures screen/system audio. Ordinary tests never access USB or start capture. Set `DYNAMICLIGHT_EXECUTABLE` to test an installed executable. The debugging port is enabled only in that test process.

## Implementation

Linux CI also exercises computer-audio input against a PulseAudio null sink, X11 capture under Xvfb, validates the udev rule, inspects the Debian package's icon/desktop entry/dependencies, and launches the packaged app on a virtual desktop. These are explicit integration tests; normal unit tests do not capture audio or a screen. Physical Linux USB, sleep, real desktop audio routing, and Wayland compositor behavior still need validation.

`src-tauri/src/linux.rs` owns the `parec` monitor subprocess (48 kHz mono float input) and systemd-logind sleep signals. Stopping capture kills and reaps the audio child before joining its reader. The sleep watcher uses a delay inhibitor when available and acknowledges the lighting worker before releasing it. Non-systemd desktops retain manual operation. X11 capture uses xcap in memory; Wayland still-image fallbacks are deliberately disabled because they may prompt repeatedly or write screenshots to disk. A future Wayland implementation should use a persistent ScreenCast portal/PipeWire stream.

- `src-tauri/src/protocol.rs` implements RB commands and SC frames, split into 64-byte payloads plus report ID 0.
- `device.rs` opens only the lighting interface, rejects the keyboard interface, checks complete writes, and drains replies. A single worker owns all USB operations.
- `engine.rs` owns settings, capture lifetimes, effects, smoothing, reconnect, and frame pacing. It does not depend on JavaScript timers. Preview events are limited to 10 Hz and suppressed while the window is unfocused, hidden, or minimized.
- `capture.rs` enumerates displays through xcap. Windows uses Desktop Duplication with direct edge sampling from a reusable GPU staging texture, with a small GDI thumbnail as a fallback for unsupported or rotated displays. macOS uses native xcap capture. `audio.rs` uses bounded sample buffers and RustFFT. The macOS bridge handles ScreenCaptureKit system audio, microphone permission, and sleep notifications.
- The HTML/CSS/JavaScript UI sends validated commands through Tauri IPC. It has no Node.js, filesystem, shell, or remote network access. Runtime assets are embedded in the executable.

Settings are atomically saved under the OS application-data directory in `com.dynamiclight.desktop/settings.json`. On first launch, valid settings from the Electron prototype's `dynamiclight/settings.json` are imported. Legacy display IDs reset to the primary display because native display IDs differ. The old file is retained.

Protocol references: [HdrBridge wire format](https://github.com/GetTheNya/HdrBridge/blob/main/node_strip_test.js), [SyncLight RB commands](https://github.com/jakebuild/synclight), and [section command investigation](https://github.com/tanmaster/light-control).
