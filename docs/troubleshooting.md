# Troubleshooting

## The strip is dark

1. Close SyncLight or any other lighting controller.
2. Check that your strip is connected and brightness is above zero.
3. Click **Color**. It connects and starts lighting automatically.
4. Click **Screen** or **Music** to connect and start capture. Switching an active display or audio source restarts capture automatically. After Stop, click **Start**. If capture needs a restart after sleep or a device change, click **Retry**.

Screen sync can be dark when the screen edges are black. Music mode is dark during silence. Protected video may not be capturable. HDR color correction is not yet implemented.

## Music is too quiet or reacts slowly

Check **Audio input** under **Diagnostics**. If no input arrives, play audio on your default speakers or select Microphone and check its permissions. Lower **Noise gate** if quiet passages disappear; raise **Sensitivity** for a stronger response. Lower **Release** and **Color smoothing** for sharper changes. Bass Pulse only responds to low frequencies—try Spectrum for a wider range of sounds.

Capture pauses if USB fails. With automatic capture resume enabled, it retries when the strip returns. The LED preview shows outgoing colors; successful USB writes cannot confirm that the physical LEDs are lit.

## Startup, wake, or reconnect

Open **Strip setup & behavior**. **Launch at login** starts the app; **Restore lighting when the app opens** separately restores a running mode. Neither option is enabled by default. Pressing **Stop** is remembered, including while disconnected or paused. Quitting preserves your last intended state for the next launch.

Color and effects resume when the computer wakes and any lock/display pause has cleared. Screen and audio capture require **Automatically resume screen and audio capture**, or a deliberate **Retry**. Automatic recovery checks existing Mac permissions without requesting them. If permission is missing, grant it in System Settings and click Retry.

USB and eligible capture recovery retry every three seconds. Stop cancels them. A selected display is never silently replaced with a different display; reconnect it or choose another. Computer audio follows the default output, and microphone capture follows the default input.

If monitoring is unavailable, a message appears under behavior settings. Linux uses logind for sleep/lock and X11 DPMS for display sleep (not Wayland). Some desktops do not publish lock state. Mac screen-lock notifications are best effort and still need hardware validation. Turning off the system-sleep blackout option leaves the last color on the controller if USB power remains available; capture always stops during suspend.

On Windows, **Launch at login** registers a Task Scheduler task (one UAC prompt), and at sign-in the app waits until the taskbar exists before showing its tray icon. Login startup runs in the tray/menu bar on Windows and Mac, and minimized on Linux. Install the app in its permanent location before enabling it. Moving the executable or AppImage may require toggling Launch at login off and on again.

## Windows: flashes, 30 FPS, or the strip stays dark

- **The strip flashes when I press Caps/Num/Scroll Lock:** Windows updates the keyboard indicators through the controller. Run DynamicLight with administrator rights (see [Windows administrator rights](installation.md#windows-administrator-rights)); without them the output is held at 30 FPS, which avoids the flashes.
- **The frame rate is limited to 30:** the app is not elevated. Use the restart-elevated button in Settings or start it from the Start menu after the installer registered the tasks.
- **Dark with the message “Another Windows user is active” or “Remote desktop session”:** the strip belongs to the session at the physical screen. DynamicLight lets go of it while another user or an RDP session is active and takes it back when you return.
- **No tray icon after sign-in:** update to 1.0.3 or newer; older versions started before Explorer was ready.
- **The strip stays lit after shutdown:** update to 1.0.4 or newer. If it still stays lit, the USB port may keep power after a fast-startup shutdown; restart instead of shutting down once and report it.
- **Windows Security removes the app or its tasks:** report the file as a false positive at https://aka.ms/wdsi (Submit a file) and include the version. Earlier versions created their tasks through `cmd.exe`/`schtasks`, which antivirus heuristics report; 1.0.2 and newer do not.

## macOS permissions

Allow DynamicLight in **System Settings → Privacy & Security → Screen & System Audio Recording** (called Screen Recording on older macOS versions). Microphone mode needs **Microphone** permission too. Restart the app after changing permission if needed.

## Linux connection or capture

See [Linux setup](linux.md) for the USB permission rule, supported desktop sessions, and audio setup.

## The window disappeared

On Windows and macOS, closing the window keeps DynamicLight in the tray/menu bar. Choose **Open DynamicLight**, or launch the app again. On Linux, closing minimizes the window instead. **Quit** stops the app.

## The colors are in the wrong place

Open **Strip setup & behavior**. Set the LED count and monitor coverage, then try **Reverse LED direction** and **Starting offset**. The default layout starts at bottom-left, goes up, then across the top and down the right edge.

## Report a problem

[Open a bug report](https://github.com/BrenOne/DynamicLight/issues/new/choose) with the app version, operating system, controller model, mode, and exact error shown. On Linux, include distribution and X11/Wayland session. Do not share device serial numbers or screenshots containing private information.
