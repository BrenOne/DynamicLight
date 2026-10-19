DynamicLight 0.16.2 adds video-area selection inside captured windows. A video playing inside a browser page can now drive the LEDs without white page margins and browser toolbars dominating the colors.

In Screen or Blend, open **Capture source → Application window**, select the browser, and click **Take window preview**. Drag around the video, then **Apply source → Start / Retry**. You can also select an area using percentages without taking a preview. Uncheck the area option for whole-window or fullscreen capture. The window follows its visible tab; after a page layout change, reselect the video area.

Previews are explicit, one-shot captures kept in memory while the source dialog is open. They do not start the lights or ongoing capture. Source bindings and crop coordinates stay local and are excluded from portable scene exports. Windows previews use the same capture backend as lighting, and undefined texture padding is excluded from both.

Validation includes native sampling and privacy tests, browser UI regression tests, and a real Windows capture fixture that reproduces white margins around an orange video, verifies the cropped colors, follows live color changes, resizes in both directions, and stops on closure. The fixture captures only its own generated window; it does not use USB or record audio.

The Windows capture initialization fix from 0.16.1 remains included, along with Studio effects and zones, RGB recording/replay, routines, independent controllers, game telemetry, declarative plugins, the CLI, and paired HTTPS phone controls.

Use **Settings → App updates → Check for updates → Install and restart** on Windows, writable macOS app installations, and Linux AppImage. Debian packages use the terminal installer or package manager. After restarting, select the intended window again and explicitly start capture. Windows selected-window capture requires Windows 10 1903 or newer.

Desktop and companion CLI downloads are available for Windows x64, macOS Apple silicon/Intel, and Linux x64. See the [installation guide](https://github.com/mskutin/DynamicLight/blob/main/docs/installation.md). Desktop updates do not update the separately installed CLI.

Updater packages are cryptographically signed; Windows publisher signing and macOS notarization remain pending. Physical hardware, mobile browser, and accessibility validation remain tracked separately from automated checks.
