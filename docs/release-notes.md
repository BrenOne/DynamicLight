DynamicLight 0.8.0 reorganizes the main window into a workspace with a mode-specific inspector, plus separate Scenes, Device setup, and Settings views. Quick controls adds one-click mode switching. Both windows share the app icon, local SVG controls, output rendering, and a subtle build-version label.

Scene editing uses a dialog, effect previews animate on hover or keyboard focus, and narrower windows prioritize controls. Brightness and Start/Stop remain available while browsing.

Download the desktop installer for your computer:

| Platform | Asset |
| --- | --- |
| Windows x64 | `dynamiclight-desktop-windows-x64-setup.exe` |
| macOS Apple silicon | `dynamiclight-desktop-macos-arm64.dmg` |
| macOS Intel | `dynamiclight-desktop-macos-x64.dmg` |
| Linux x64 (Ubuntu 24.04+) | `dynamiclight-desktop-linux-x64.deb` or `dynamiclight-desktop-linux-x64.AppImage` |

Terminal installation is supported by the attached `install.sh` (macOS/Linux) and `install.ps1` (Windows). See the [installation guide](https://github.com/mskutin/DynamicLight/blob/main/docs/installation.md). `SHA256SUMS` covers every package, installer script and the Linux USB permission rule. GitHub's automatically generated source archives are source code, not installers.

These are unsigned preview builds. macOS requires 13 or newer; Linux screen sync requires X11. Mac and Linux physical hardware validation remains pending. Installation does not enable login startup or start lighting/capture. Command-line lighting control is planned separately in issue #16.
