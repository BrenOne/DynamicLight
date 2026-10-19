# Install DynamicLight

Download a package from [Releases](https://github.com/mskutin/DynamicLight/releases/latest). No GitHub sign-in or development tools are needed. Choose the `.exe` for Windows x64, the ARM64 or x64 `.dmg` for your Mac, or the x64 `.deb`/`.AppImage` for Linux. The source-code ZIP and tarball are not installable apps.

Builds are unsigned previews. Windows may show SmartScreen; macOS may require approval in **System Settings → Privacy & Security** after you try to open the app. The installers do not disable OS security checks. macOS requires 13 or newer; Linux binaries target Ubuntu 24.04+ and screen sync needs X11. Windows ARM64/32-bit and Linux ARM64 do not currently have release packages.

## From a terminal

Quit DynamicLight before installing or updating. Run these commands as your normal user. Downloading the installer to a file lets you inspect it before running it.

**Windows (PowerShell 5.1 or newer):**

```powershell
Invoke-WebRequest https://github.com/mskutin/DynamicLight/releases/latest/download/install.ps1 -OutFile install.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File .\install.ps1
```

This runs the per-user Windows installer silently and waits for completion. It does not launch the app. The execution-policy setting applies only to that PowerShell process. Open DynamicLight from the Start menu afterward.

**macOS or Linux (Bash and curl):**

```sh
curl -fsSL https://github.com/mskutin/DynamicLight/releases/latest/download/install.sh -o install.sh
bash install.sh
```

On Mac this installs the native app into `~/Applications/DynamicLight.app`, including when your terminal runs under Rosetta. Open it from Finder afterward. On Linux it uses `sudo apt-get install` for the Debian package, including dependencies and USB rules. Reconnect the strip after installation. The same commands update the app and preserve settings.

For a user-local Linux AppImage:

```sh
bash install.sh --appimage
```

This installs `~/.local/opt/dynamiclight-desktop/DynamicLight.AppImage` and downloads the USB rule beside it. Complete the one-time [Linux dependency and USB setup](linux.md#appimage), then run that file. Other distributions must supply compatible runtime libraries or build from source; AppImage does not remove the Ubuntu 24.04/glibc compatibility floor.

## Choose a release or download without installing

By default, the scripts resolve the latest non-prerelease once, then download all files from that exact version. To pin a release (including a prerelease), use `--version 0.7.0` in Bash or `-Version 0.7.0` in PowerShell. An optional `v` prefix is accepted. To pin the script itself, replace `/latest/download/` in its URL with `/download/v0.7.0/`.

```sh
bash install.sh --version 0.7.0 --download-only --directory ./downloads
```

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\install.ps1 -Version 0.7.0 -DownloadOnly -Directory .\downloads
```

The installers verify SHA-256 before installation and stop on download failure, unsupported architecture, missing checksum or checksum mismatch. Checksums detect corrupt or inconsistent downloads; they are not a substitute for publisher code signing. Manual downloads can be verified against the release's `SHA256SUMS` with `sha256sum`, `shasum -a 256` or `Get-FileHash -Algorithm SHA256`.

## In-app updates

Install 0.9.0 or newer once using the downloads or terminal commands above. Earlier versions have no updater. In **Settings → App updates**, automatic checks are enabled by default, run 30 seconds after startup and every six hours, and can be turned off. **Check for updates** also works manually. Checks contact GitHub; no lighting settings, profiles, screen content or audio are uploaded.

When an update is available, choose **Install and restart** and confirm. The app saves pending settings, downloads the package, and verifies its signature and signed version before releasing USB and capture resources. It then installs and restarts. Your existing restore-on-start and capture consent preferences control whether lighting resumes. A failed download leaves lighting running; after an installer failure you can start lighting again and retry.

Windows NSIS installations, writable macOS `.app` installations (copy out of the DMG first), and Linux AppImages support this flow. Debian packages and development builds show manual installation guidance. Rerunning the terminal installer remains supported on every platform. AppImage updates do not install system USB rules or dependencies.

The updater uses a dedicated signing key embedded in the app. This verification is separate from Windows Authenticode and macOS Developer ID/notarization; the preview packages still lack those OS signatures. PR artifacts use disposable signing keys and cannot accept production updates; install a GitHub Release for normal use.

## Command-line controls and uninstall

Use Windows **Installed apps**, delete `~/Applications/DynamicLight.app` on Mac, or follow [Linux uninstall instructions](linux.md). Saved preferences are retained.

Add `--cli` in Bash or `-Cli` in PowerShell to install the standalone CLI in `~/.local/bin` or `%LOCALAPPDATA%\DynamicLightCLI\bin`. Controls require the desktop app running with **Settings → Local automation** enabled. Installers do not alter PATH or install a background service. See the [automation guide](automation.md) for commands, examples, and CLI updates/uninstallation.
