DynamicLight 0.10.0 adds an opt-in authenticated local API and a standalone CLI for Windows, macOS, and Linux. Enable Settings → Local automation, then use `dynamiclight status`, `power on|off|toggle`, `brightness 35`, `scenes list|apply`, or `profiles list|apply`.

The API binds only to localhost, uses a private rotating credential, and validates and rate-limits requests. Capture starts require both the Settings permission and explicit command consent. Commands use the existing lighting engine, preserving comfort limits, lifecycle rules, and capture permission checks.

| Platform | Desktop asset | CLI asset |
| --- | --- | --- |
| Windows x64 | `dynamiclight-desktop-windows-x64-setup.exe` | `dynamiclight-cli-windows-x64.exe` |
| macOS Apple silicon | `dynamiclight-desktop-macos-arm64.dmg` | `dynamiclight-cli-macos-arm64` |
| macOS Intel | `dynamiclight-desktop-macos-x64.dmg` | `dynamiclight-cli-macos-x64` |
| Linux x64 (Ubuntu 24.04+) | `dynamiclight-desktop-linux-x64.deb` or `.AppImage` | `dynamiclight-cli-linux-x64` |

Terminal installers install the desktop by default. Use `bash install.sh --cli` or `install.ps1 -Cli` for the CLI. `SHA256SUMS` covers all assets. The CLI requires the desktop app to remain running and is updated separately by rerunning its installer. See the [installation guide](https://github.com/mskutin/DynamicLight/blob/main/docs/installation.md) and [automation guide](https://github.com/mskutin/DynamicLight/blob/main/docs/automation.md) for shell, Stream Deck, and home-automation examples.

Existing 0.9.0 installations can use Settings → App updates. Earlier versions need a manual installation once. Desktop updater packages remain cryptographically signed. OS publisher signing/notarization is still pending; these are preview builds. macOS requires 13+, and Linux screen sync requires X11. Physical Mac/Linux hardware and Stream Deck validation remain pending. Installation does not start lighting or capture.
