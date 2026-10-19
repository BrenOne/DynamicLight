DynamicLight 0.16.1 fixes Windows selected-window capture failing with “Cannot change thread mode after it is set” (0x80010106) after audio initialization.

Each lighting worker now initializes the Windows Runtime before audio discovery and keeps it initialized across capture sessions. Switching between Music, Screen, and Blend no longer creates that thread-mode conflict. Unrelated capture errors also no longer incorrectly suggest that your Windows version is too old.

Validation includes 100 passing native tests, a regression test verified to fail before the fix, and a real Windows capture fixture covering audio-device discovery, repeated capture startup/teardown, resizing, and window closure. The fixture records only its own generated window; it does not use USB or record audio.

All features from 0.16.0 remain available, including Studio effects and zones, RGB recording/replay, routines, independent controllers, game telemetry, declarative plugins, the CLI, and paired HTTPS phone controls.

Use **Settings → App updates → Check for updates → Install and restart** on Windows, writable macOS app installations, and Linux AppImage. Debian packages use the terminal installer or package manager. After restarting, select the intended window again and explicitly start capture. Windows selected-window capture requires Windows 10 1903 or newer.

Desktop and companion CLI downloads are available for Windows x64, macOS Apple silicon/Intel, and Linux x64. See the [installation guide](https://github.com/mskutin/DynamicLight/blob/main/docs/installation.md). Desktop updates do not update the separately installed CLI.

Updater packages are cryptographically signed; Windows publisher signing and macOS notarization remain pending. Physical hardware, mobile browser, and accessibility validation remain tracked separately from automated checks.
