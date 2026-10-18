# Tauri migration validation

The 0.2.x migration replaces Electron and node-hid with Tauri 2, Rust hidapi, native screen capture, and native audio capture. It includes Windows, macOS, and Linux packages. The existing studio controls remain. Screen preview now shows sampled edge colors, rather than a copy of the captured desktop.

## Automated coverage

Fifteen Rust tests cover observed initialization packets, solid range clearing, report reassembly at 1/65/127 LEDs, rejection of the keyboard interface, incomplete/disconnected writes, spatial sampling (including BGRA row padding), smoothing decay, bounded audio buffering, FFT frequency localization, invalid settings, atomic settings replacement, and capture/sleep state transitions. Clippy passes with warnings denied.

## Platform checks

Windows release executable tested against the connected 65-LED ROBOBLOQ controller:

- UI launches in WebView2 with no JavaScript page errors; invalid IPC settings and internal lifecycle commands are rejected.
- Solid color, power toggles, 3840×2160 screen capture, system-audio capture, and color flow passed integration checks.
- Installed 0.2.1 starts capture from disconnected/power-off in one click and labels stopped capture explicitly. Windows startup sets and reads back the separate taskbar icon handle.
- After capture startup, screen sync sustained 29–30 FPS. System audio and color flow also reached 29 FPS; a typical audio check received more than 70,000 mono samples in 1.5 seconds. The user confirmed the physical strip lit up during screen sync.
- The native worker submitted 59–60 frames while the test froze JavaScript for two seconds.
- Quit completed cleanly through the application's shutdown handler.
- NSIS installer is approximately 2.31 MiB; the application executable is approximately 10 MiB. The previous Electron portable package was approximately 96.8 MiB. These are different package formats; WebView2 is supplied separately by Windows and is not included in those Tauri sizes.

Windows, macOS, and Linux CI builds are tracked on the migration PR. Linux CI includes PulseAudio null-sink capture and child cleanup, X11 capture under Xvfb, udev rule validation, Debian package inspection, and packaged startup on a virtual desktop. Physical Linux/macOS USB, capture permissions, audio routing, sleep/wake, microphone input, and physical unplug/replug validation remain necessary. Simulated lifecycle tests do not replace physical suspend testing.

Screen capture uses ordinary RGB and does not apply HDR tone mapping. macOS requires 13+ for system audio. Linux packages target Ubuntu 24.04+ x86-64; screen sync requires X11 and Wayland screen streaming is not implemented. Linux computer audio needs PulseAudio or pipewire-pulse; sleep handling needs systemd-logind. Default audio-device changes require stopping and restarting capture. No signed distribution or automatic updater is configured.
