# DynamicLight

Control compatible USB LED strips with fixed colors, screen sync, music, and animated effects.

**Windows · macOS · Linux** — free, open source, and local. No account required.

![DynamicLight lighting studio](docs/studio.png)

## Get the app

DynamicLight is in preview. Download a build from [GitHub Actions](https://github.com/mskutin/DynamicLight/actions/workflows/build.yml): open a successful run, scroll to **Artifacts**, and choose your platform. GitHub requires sign-in to download these preview builds.

| Your computer | Download and open |
| --- | --- |
| Windows | Windows artifact → `.exe` installer |
| Mac with Apple silicon | `aarch64-apple-darwin` artifact → `.dmg` |
| Mac with Intel | `x86_64-apple-darwin` artifact → `.dmg` |
| Linux | Linux artifact → `.deb` or `.AppImage`; see [Linux setup](docs/linux.md) |

macOS needs version 13 or newer. Linux builds target Ubuntu 24.04+; screen sync currently needs X11. Preview builds are unsigned. Windows has been tested with a physical strip; Mac and Linux hardware testing is still pending.

## Getting started

1. Plug in your USB strip and close SyncLight.
2. Click **Color**, **Screen**, **Music**, **Blend**, or **Effects**. The app connects and starts that mode in one action.
3. Use **Stop** to stop lighting, or **Start** to resume the selected mode.
4. Adjust brightness. Use **Strip setup & behavior → Calibrate strip** to check LED count, direction, and placement.

| Mode | What it does |
| --- | --- |
| Color | One color, at your chosen brightness |
| Screen | Colors from the edges of your screen |
| Music | Spectrum, bass pulse, beat pulse, level meter, or color wave from computer audio or your microphone |
| Blend | Screen colors with audio-reactive brightness; silence keeps normal screen lighting |
| Effects | Color flow, aurora, breathe, comet, or ember, with adjustable speed and direction |

Each mode remembers its brightness, colors, sources, and tuning when you switch away and return. Strip layout, brightness limits, and startup preferences stay shared.

Effect thumbnails animate using the same native patterns as the strip, including your palette, speed, and brightness. They pause when hidden and respect reduced-motion settings.

Tune music with input gain, sensitivity, a noise gate, and fade-out controls. Choose an Aurora, Ocean, Sunset, or custom-color palette. Changes reach the lighting engine while you drag; settings save when you pause.

Use **Color & palettes** for a color wheel, RGB/hex input, approximate white temperature, and named palettes of up to eight colors. Preview changes before applying them. Custom palettes work with music, Aurora, Breathe, and Comet.

Open **Profiles & scenes** to browse starter scenes or save settings for work, films, or games. Apply a profile in one click, add it to tray favorites, or assign a shortcut such as `Control+Shift+1` (`Command+Shift+1` on Mac). Selecting a capture profile starts its screen/audio source and may request OS permission. Profiles preserve your strip layout, brightness limits, and startup preferences.

Search by name or mode, filter favorites, or choose **Save a copy** to customize Warm desk, Cinema, Neon, or Ocean. Scene thumbnails are simulated examples; they never start screen or audio capture.

Use **Export** to share selected profiles and palettes, or **Import** to review a scene file and keep both, replace, or skip matching items. Device bindings and shortcuts stay local. Importing saves a backup and leaves the current lighting alone.

Open **Quick controls** from the header or tray for a small window with Start/Stop, live brightness, favorites, and Retry. You can keep it on top or return to the studio. Closing it leaves lighting running.

The tray/menu bar offers Start/Stop, brightness, recent colors, favorite profiles, and Retry. Changes also appear in the main window. Linux global shortcuts need X11/XWayland support; if registration is unavailable, use the app or tray.

In Screen and Blend, **Screen color adjustments** offers black-bar detection, RGB balance, saturation, and gamma. **Brightness limits** can cap output, soften abrupt changes, or keep a small ambient glow. Stop always turns lighting off immediately. HDR and protected video can differ from the picture you see.

Calibration temporarily pauses the current scene. Step through individual LEDs, preview the mapped edges, then apply or cancel to restore the scene. An unattended test stops after five minutes. In Blend, losing one capture source uses the remaining source and shows Retry; losing both stops output until recovery.

The live canvas previews outgoing LED colors. **Diagnostics** shows USB frame rate, successful writes, audio input, and missed animation deadlines. Open **Troubleshooting report** to preview, copy, or save a report to Downloads. Optional local history keeps up to 200 state changes for seven days; disable it or choose Clear history to erase it. Reports exclude paths, identifiers, profile names, screen content, and audio. The controller cannot report whether the physical LEDs are lit.

Closing the window keeps lighting running. Choose **Quit** to exit.

Under **Strip setup & behavior**, you can enable **Launch at login** and **Restore lighting when the app opens**. Both are off initially. Color and effects resume after wake or USB reconnect; screen and music capture need **Retry** unless you enable **Automatically resume screen and audio capture**. **Stop** cancels recovery and keeps lighting off on the next launch.

Display sleep, computer lock, and system sleep have separate controls. Linux display-only sleep detection needs X11; lock/suspend handling needs logind and desktop support. Mac lock detection and the new lifecycle flows still need physical validation on each platform.

## Does it work with my strip?

DynamicLight supports the SyncLight-compatible **ROBOBLOQ USB controller (1A86:FE07)**, with 1–127 LEDs. A USB plug alone does not mean a strip is compatible.

Lights still dark? Check brightness and any error shown when starting. See [troubleshooting](docs/troubleshooting.md) or [report a problem](https://github.com/mskutin/DynamicLight/issues/new/choose).

[Planned features](BACKLOG.md) · [Developer guide](docs/development.md) · [Contributing](CONTRIBUTING.md)

Released under the [MIT License](LICENSE). Copyright © 2026 mskutin.
