# Troubleshooting

## The strip is dark

1. Close SyncLight or any other lighting controller.
2. Check that your strip is connected and brightness is above zero.
3. Click **Color**. It connects and starts lighting automatically.
4. Click **Screen** or **Music** to connect and start capture. Switching an active display or audio source restarts capture automatically. After Stop or sleep, click **Start**.

Screen sync can be dark when the screen edges are black. Music mode is dark during silence. Protected video may not be capturable. HDR color correction is not yet implemented.

## Music is too quiet or reacts slowly

Check **Audio input** under **Diagnostics**. If no input arrives, play audio on your default speakers or select Microphone and check its permissions. Lower **Noise gate** if quiet passages disappear; raise **Sensitivity** for a stronger response. Lower **Release** and **Color smoothing** for sharper changes. Bass Pulse only responds to low frequencies—try Spectrum for a wider range of sounds.

The frequency bars show analyzed audio, even while the strip is disconnected. The LED preview shows outgoing colors; successful USB writes cannot confirm that the physical LEDs are lit.

## macOS permissions

Allow DynamicLight in **System Settings → Privacy & Security → Screen & System Audio Recording** (called Screen Recording on older macOS versions). Microphone mode needs **Microphone** permission too. Restart the app after changing permission if needed.

## Linux connection or capture

See [Linux setup](linux.md) for the USB permission rule, supported desktop sessions, and audio setup.

## The window disappeared

On Windows and macOS, closing the window keeps DynamicLight in the tray/menu bar. Choose **Open DynamicLight**, or launch the app again. On Linux, closing minimizes the window instead. **Quit** stops the app.

## The colors are in the wrong place

Open **Strip setup & behavior**. Set the LED count and monitor coverage, then try **Reverse LED direction** and **Starting offset**. The default layout starts at bottom-left, goes up, then across the top and down the right edge.

## Report a problem

[Open a bug report](https://github.com/mskutin/DynamicLight/issues/new/choose) with the app version, operating system, controller model, mode, and exact error shown. On Linux, include distribution and X11/Wayland session. Do not share device serial numbers or screenshots containing private information.
