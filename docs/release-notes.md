DynamicLight 1.1.1 is a release of the BrenOne fork of mskutin/DynamicLight, now for Windows, macOS and Linux.

**New in 1.1.1**

- Windows: updating the app no longer removes the start-with-Windows entry or the administrator-rights tasks (the uninstall step now leaves them alone during updates).
- Windows: switching **Run with administrator rights** is more reliable. If the old Run entry cannot be removed, or Launch at login cannot be restored, you get a clear error instead of two copies starting at sign-in; the settings always show the real state afterwards.
- Screen capture returns from the GDI fallback faster after a long lock screen or UAC prompt.

**Highlights of this fork**

- Stable Robobloq output: steady USB frames, automatic strip-layout detection and up to 60 FPS.
- Windows: no flashes when Caps/Num/Scroll Lock changes. This needs administrator rights; without them output is capped at 30 FPS, which is always stable.
- Windows: the elevated start and **Launch at login** use Task Scheduler tasks created through the Windows API after one UAC approval, per Windows user. No `cmd.exe`/`schtasks` command lines, which antivirus heuristics report as persistence malware.
- Windows: at sign-in the app waits for the taskbar, so the tray icon always appears.
- Windows: with several users or a remote desktop session, the app lets go of the strip while another session has the screen.

Use **Settings → App updates → Check for updates → Install and restart** on Windows, writable macOS app installations, and Linux AppImage. Debian packages use the terminal installer or package manager. Versions before 0.9.0 need one manual upgrade to gain the updater.

Desktop and companion CLI downloads are available for Windows x64, macOS Apple silicon/Intel, and Linux x64. See the [installation guide](https://github.com/BrenOne/DynamicLight/blob/main/docs/installation.md). Desktop updates do not update the separately installed CLI.

Updater packages are cryptographically signed; Windows publisher signing and macOS notarization are not available, so the OS may ask you to confirm opening the app. Physical hardware, mobile browser and accessibility validation remain tracked separately from automated checks.
