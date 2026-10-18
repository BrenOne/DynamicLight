# DynamicLight

A Windows/macOS desktop controller for SyncLight-compatible USB monitor backlights. No account, server, cloud connection, or original SyncLight installation is required at runtime.

**Early prototype:** Windows USB control and capture have been tested. macOS hardware validation is still pending.

![DynamicLight lighting studio](docs/studio.png)

[Feature and UI backlog](BACKLOG.md) · [Issues](https://github.com/mskutin/DynamicLight/issues) · [Builds](https://github.com/mskutin/DynamicLight/actions/workflows/build.yml) · [Contributing](CONTRIBUTING.md)

## Use the Windows build

Build locally with `pnpm run build:win`, or download a successful Windows workflow artifact from [GitHub Actions](https://github.com/mskutin/DynamicLight/actions/workflows/build.yml). Open the resulting `DynamicLight 0.1.0.exe`. Exit SyncLight first, then click **Connect strip**.

- **Solid color:** pick a color and adjust brightness.
- **Screen sync:** choose a display using Refresh, then click Start capture.
- **Music:** select Computer audio or Microphone, then Start capture. Computer audio reacts to playback; microphone mode listens to the room.
- **Color flow:** a continuously moving spectrum.
- **Strip setup:** adjust the LED count, three/four-sided layout, direction, starting offset, smoothing, and frame rate.
- Closing the window keeps the app running in the tray/menu bar. Use the tray menu to quit.

The initial profile is 65 LEDs, three sides, RGB, 38% brightness. It was chosen from the connected strip's existing SyncLight configuration. Change these settings for another installation. Layout starts at bottom-left, travels up the left edge, across the top, and down the right. Four-sided mode then crosses the bottom. Physical spacing is approximated using the display aspect ratio.

The app connects only after you click Connect. After connecting, it retries a lost USB connection every three seconds. Screen/audio capture always requires an explicit Start capture action. After computer sleep, turn the lights on and restart capture. This version responds to computer suspend, not display-only sleep.

## Develop

Install Node.js 24 LTS, then from this folder:

```sh
npm install
npm start
```

Alternatively use pnpm 11+ (`pnpm install`, `pnpm start`). `pnpm-lock.yaml` records the tested dependency versions. Electron downloads its platform binary during installation or first launch. The node-hid dependency ships native bindings for Windows x64/ARM64 and macOS Intel/Apple Silicon.

```sh
npm test           # Protocol, spatial sampling, settings, and driver recovery tests
npm run devices    # Read-only listing of compatible USB interfaces
npm run build:win  # Windows portable executable
npm run build:mac  # macOS DMGs for Apple Silicon and Intel; run on a Mac
```

Use the build command on its respective OS. For repeatable CI, `.github/workflows/build.yml` builds Windows and both Mac architectures and retains artifacts. It runs manually or on pushes to `main`. A successful build does not replace hardware validation.

Builds are unsigned unless signing credentials are configured. For distribution to other people, add Windows signing and Apple Developer signing/notarization. The local Windows build is suitable for testing on this computer.

On macOS, enable DynamicLight in **System Settings → Privacy & Security → Screen & System Audio Recording** for screen sync and computer audio. Microphone mode also needs microphone permission. Restart after changing permissions. System-audio support depends on the macOS/Electron combination; use Microphone if loopback capture is unavailable. No microphone monitoring is played through your speakers.

## Validation performed on Windows

- USB identity: `ROBOBLOQ`, VID `1A86`, PID `FE07`, lighting interface 0.
- Solid color and complete 65-LED streaming frames accepted by the device. The owner confirmed the physical purple color change.
- Screen capture opened at 640×360; the integration test observed 30 FPS of successful USB frame submissions.
- System-audio capture returned a live audio track and running AudioContext. Actual music responsiveness and microphone input need listening/visual testing.
- Color flow observed at 29–30 FPS of successful USB frame submissions.
- Nine automated tests passed; desktop UI smoke and hardware integration runs had no JavaScript page errors.
- The packaged Windows application passed the same USB, screen-capture, audio-capture, and power-toggle integration check.

macOS packaging was attempted on Windows and refused by Electron Builder because it requires a Mac. No Mac installer has been produced yet. macOS runtime, unplug/replug on physical hardware, sleep/wake, and HDR/DRM content have not been hardware-tested. Recovery is covered with a simulated HID device. Screen sampling currently uses ordinary captured RGB; this is not an HDR-aware color pipeline. Protected video may capture as black. This first version supports one controller and 1–127 LEDs, with RGB wiring.

## Implementation

`src/protocol.cjs` implements RB commands and SC streaming packets. Reports use 64 payload bytes plus report ID 0. `src/controller.cjs` opens only the lighting interface, checks complete writes, and permits one write at a time. The renderer awaits each frame rather than queuing a growing backlog. The main process validates IPC data; the renderer is sandboxed and has no Node.js access. Settings are atomically saved under Electron's per-user application-data folder.

The app has no network requests. Capture data is reduced to colors in memory and is not saved or uploaded. The test screenshots contain only the application's UI.

Protocol references used to independently implement the driver:

- [HdrBridge wire format and initialization](https://github.com/GetTheNya/HdrBridge/blob/main/node_strip_test.js)
- [SyncLight RB command documentation](https://github.com/jakebuild/synclight)
- [light-control section command investigation](https://github.com/tanmaster/light-control)
- [node-hid API](https://github.com/node-hid/node-hid)
- [Electron screen and system-audio capture](https://www.electronjs.org/docs/latest/api/desktop-capturer)

`scripts/smoke.cjs` tests the UI without USB or capture. `scripts/hardware-smoke.cjs` explicitly connects, changes lighting, and briefly captures screen/system audio. These developer scripts require Playwright, either installed locally or supplied via the `PLAYWRIGHT_PATH` environment variable. They are excluded from the packaged app.

## License

Copyright (c) 2026 mskutin. Released under the [MIT License](LICENSE). Third-party dependencies retain their own licenses.
