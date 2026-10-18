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
2. Click **Color**, **Screen**, **Music**, or **Effects**. The app connects and starts that mode in one action.
3. Use **Stop** to stop lighting, or **Start** to resume the selected mode.
4. Adjust brightness. Open **Strip setup & behavior** if you need to change the number or order of LEDs.

| Mode | What it does |
| --- | --- |
| Color | One color, at your chosen brightness |
| Screen | Colors from the edges of your screen |
| Music | Spectrum, bass pulse, level meter, or color wave from computer audio or your microphone |
| Effects | Color flow, aurora, breathe, comet, or ember, with adjustable speed |

Effect thumbnails animate using the same native patterns as the strip, including your palette, speed, and brightness. They pause when hidden and respect reduced-motion settings.

Tune music with sensitivity, a noise gate, and fade-out controls. Choose an Aurora, Ocean, Sunset, or custom-color palette. Changes reach the lighting engine while you drag; settings save when you pause.

The live canvas previews outgoing LED colors. **Diagnostics** shows USB frame rate, successful writes, and audio input. The controller cannot report whether the physical LEDs are lit.

Closing the window keeps lighting running. Choose **Quit** to exit.

Under **Strip setup & behavior**, you can enable **Launch at login** and **Restore lighting when the app opens**. Both are off initially. Color and effects resume after wake or USB reconnect; screen and music capture need **Retry** unless you enable **Automatically resume screen and audio capture**. **Stop** cancels recovery and keeps lighting off on the next launch.

Display sleep, computer lock, and system sleep have separate controls. Linux display-only sleep detection needs X11; lock/suspend handling needs logind and desktop support. Mac lock detection and the new lifecycle flows still need physical validation on each platform.

## Does it work with my strip?

DynamicLight supports the SyncLight-compatible **ROBOBLOQ USB controller (1A86:FE07)**, with 1–127 LEDs. A USB plug alone does not mean a strip is compatible.

Lights still dark? Check brightness and any error shown when starting. See [troubleshooting](docs/troubleshooting.md) or [report a problem](https://github.com/mskutin/DynamicLight/issues/new/choose).

[Planned features](BACKLOG.md) · [Developer guide](docs/development.md) · [Contributing](CONTRIBUTING.md)

Released under the [MIT License](LICENSE). Copyright © 2026 mskutin.
