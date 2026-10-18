# DynamicLight

Make your monitor backlight your own. Pick a color, follow the screen, or let your lights move with music.

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

## Light it up

1. Plug in your USB strip and close SyncLight.
2. Open DynamicLight and choose a mode.
3. For **Solid color** or **Color flow**, click **Connect strip**. For **Screen sync** or **Music**, click **Start capture**—it connects and turns on the strip for you.
4. Adjust brightness. Open **Strip setup & behavior** if you need to change the number or order of LEDs.

| Mode | What it does |
| --- | --- |
| Solid color | One color, at your chosen brightness |
| Screen sync | Colors from the edges of your screen |
| Music | Lighting that reacts to computer audio or your microphone |
| Color flow | A slowly moving rainbow |

Closing the window keeps lighting running. Choose **Quit DynamicLight** to exit. After sleep, click **Start capture** again to resume screen or music lighting.

## Does it work with my strip?

DynamicLight supports the SyncLight-compatible **ROBOBLOQ USB controller (1A86:FE07)**, with 1–127 LEDs. A USB plug alone does not mean a strip is compatible.

Lights still dark? Check brightness and power; screen and music modes need capture running. See [troubleshooting](docs/troubleshooting.md) or [report a problem](https://github.com/mskutin/DynamicLight/issues/new/choose).

[Planned features](BACKLOG.md) · [Developer guide](docs/development.md) · [Contributing](CONTRIBUTING.md)

Released under the [MIT License](LICENSE). Copyright © 2026 mskutin.
