DynamicLight 0.9.0 adds signed in-app updates. Settings → App updates checks GitHub automatically (with an opt-out), shows release notes and download progress, and offers Install and restart after confirmation. Windows, macOS and Linux AppImage packages can update in place; Debian users keep using their terminal installer or package manager.

Downloads and their signed version are verified before lighting stops. Settings and existing startup/capture consent are preserved. Earlier versions do not contain an updater: install 0.9.0 manually once to receive future updates in the app.

Download the desktop installer for your computer:

| Platform | Asset |
| --- | --- |
| Windows x64 | `dynamiclight-desktop-windows-x64-setup.exe` |
| macOS Apple silicon | `dynamiclight-desktop-macos-arm64.dmg` |
| macOS Intel | `dynamiclight-desktop-macos-x64.dmg` |
| Linux x64 (Ubuntu 24.04+) | `dynamiclight-desktop-linux-x64.deb` or `dynamiclight-desktop-linux-x64.AppImage` |

Terminal installation is supported by the attached `install.sh` (macOS/Linux) and `install.ps1` (Windows). See the [installation guide](https://github.com/mskutin/DynamicLight/blob/main/docs/installation.md). `SHA256SUMS` covers every package, installer script and the Linux USB permission rule. GitHub's automatically generated source archives are source code, not installers.

These are unsigned preview builds. macOS requires 13 or newer; Linux screen sync requires X11. Mac and Linux physical hardware validation remains pending. Installation does not enable login startup or start lighting/capture. Command-line lighting control is planned separately in issue #16.
