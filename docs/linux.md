# Linux setup

Linux support is a preview. The downloadable x86-64 builds target **Ubuntu 24.04 or newer**. Other distributions can build from source using the [developer guide](development.md). Physical USB and sleep/wake validation on Linux is still pending.

| Feature | Availability |
| --- | --- |
| Color and all five effects | X11 and Wayland desktops |
| Screen sync | X11 only; choose Xorg/X11 at the login screen |
| Computer audio | PulseAudio, or PipeWire with `pipewire-pulse` |
| Microphone | Default ALSA input; desktop audio routing may require `libasound2-plugins` |
| Automatic lights off on suspend | systemd-logind; best effort if unavailable |
| Lock/unlock | logind LockedHint, when published by the desktop |
| Display-only sleep | X11 DPMS; unavailable on Wayland |
| Login startup | Opt-in, using desktop autostart |

Wayland screen sync is not enabled yet. Other modes remain available. Closing the app window minimizes it, so you can reopen it even on desktops without tray icons.

## Recommended: Debian package

Download the `.deb` from [Releases](https://github.com/mskutin/DynamicLight/releases/latest), then install it from your Downloads directory:

```sh
sudo apt install ./dynamiclight-desktop-linux-x64.deb
```

The package installs the required libraries, audio helper, app icon, and a USB permission rule. **Unplug and reconnect the strip after installation**, then launch DynamicLight from your applications menu. Run the app as your normal user.

## AppImage

The AppImage needs the host's audio service and the USB permission rule. Download `69-dynamiclight.rules` from the same release and install it once. If you used `bash install.sh --appimage`, first change to `~/.local/opt/dynamiclight-desktop`, where the installer saved the rule:

```sh
sudo apt install pulseaudio-utils libasound2-plugins libfuse2t64
sudo install -m 644 69-dynamiclight.rules /etc/udev/rules.d/69-dynamiclight.rules
sudo udevadm control --reload-rules
```

Unplug and reconnect the strip. Mark the AppImage executable in its file properties, or run:

```sh
chmod +x dynamiclight-desktop-linux-x64.AppImage
./dynamiclight-desktop-linux-x64.AppImage
```

The terminal installer instead names the installed file `~/.local/opt/dynamiclight-desktop/DynamicLight.AppImage`; run that path directly. The rule grants only the active local user access to this controller's lighting interface; it does not grant access to all USB devices or the keyboard interface. AppImage updates do not install system permission rules automatically.

## If something does not work

- **Cannot connect:** replug after installing the permission rule, close other strip controllers, and check that your controller is `1a86:fe07` using `lsusb`.
- **Screen sync unavailable:** check your session with `echo "$XDG_SESSION_TYPE"`. Use X11 for screen sync; Wayland streaming support remains planned.
- **Computer audio unavailable:** `pactl info` should connect to your desktop audio service and `parec --version` should work. Install `pulseaudio-utils` and enable your distribution's PulseAudio or `pipewire-pulse` service. Enable automatic capture resume to follow default-output changes, or click Retry after a change.
- **No tray icon:** reopen the minimized app from the taskbar or applications menu. GNOME may require an AppIndicator extension to show tray icons, but the app does not require one.
- **Lights stay on during suspend:** automatic sleep handling requires systemd-logind and permission for a delay inhibitor. Turn lights off manually if your desktop does not provide these.

To uninstall a Debian package, use `sudo apt remove dynamiclight`. For an AppImage, delete the AppImage (or the terminal installer's `~/.local/opt/dynamiclight-desktop` directory) and, if no longer needed, the manually installed `/etc/udev/rules.d/69-dynamiclight.rules` file; reload udev rules and reconnect the strip afterward. Saved lighting preferences are retained.
