DynamicLight 1.0.4 is a release of the BrenOne fork of mskutin/DynamicLight, now for Windows, macOS and Linux.

**New in 1.0.4**

- Windows: the strip is switched off when Windows shuts down, restarts or signs out. It used to stay lit with the last frame.
- Packages for macOS (Apple silicon and Intel) and Linux x64 are built and published together with Windows. If the macOS or Linux build fails, the release goes out without it and says so. macOS and Linux are experimental: they are not yet tested with a real controller.
- `install.sh` and `install.ps1` download from this fork's releases.

**Highlights of this fork**

- Stable Robobloq output: steady USB frames, automatic strip-layout detection and up to 60 FPS.
- Windows: no flashes when Caps/Num/Scroll Lock changes. This needs administrator rights; without them output is capped at 30 FPS, which is always stable.
- Windows: the elevated start and **Launch at login** use Task Scheduler tasks created through the Windows API after one UAC approval, per Windows user. No `cmd.exe`/`schtasks` command lines, which antivirus heuristics report as persistence malware.
- Windows: at sign-in the app waits for the taskbar, so the tray icon always appears.
- Windows: with several users or a remote desktop session, the app lets go of the strip while another session has the screen.

Use **Settings → App updates → Check for updates → Install and restart** on Windows, writable macOS app installations, and Linux AppImage. Debian packages use the terminal installer or package manager. Versions before 0.9.0 need one manual upgrade to gain the updater.

Desktop and companion CLI downloads are available for Windows x64, macOS Apple silicon/Intel, and Linux x64. See the [installation guide](https://github.com/BrenOne/DynamicLight/blob/main/docs/installation.md). Desktop updates do not update the separately installed CLI.

Updater packages are cryptographically signed; Windows publisher signing and macOS notarization are not available, so the OS may ask you to confirm opening the app. Physical hardware, mobile browser and accessibility validation remain tracked separately from automated checks.
