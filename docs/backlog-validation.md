# Validation handoff through 0.16

All implementation features are delivered. Physical validation is still pending in [Windows/Linux #45](https://github.com/mskutin/DynamicLight/issues/45), [Intel/Apple Silicon Mac #38](https://github.com/mskutin/DynamicLight/issues/38), and [accessibility #37](https://github.com/mskutin/DynamicLight/issues/37). Existing [lifecycle](lifecycle-testing.md) and [studio](studio-testing.md) checks still apply. Record exact release, OS/desktop/session, device/interface, monitor layout, audio route and phone/browser where applicable; link failures and retest fixes. Do not infer a pass from CI or a simulation.

| Feature | Additional physical checks | Current result |
| --- | --- | --- |
| Studio compositions and zones | Layer order, repeat/hold, fades, range overlaps, count/remapping, physical first LED, reverse/offset and comfort limits | Pending |
| Bias lighting and image palettes | RGB approximation versus measured reference; saved calibration; palette order and local image import; no claimed color accuracy without measurement | Pending |
| Preview and appearance | Pointer and keyboard LED/range selection; Apply/Cancel/undo; theme, accent and density across main/tray/Quick controls at high DPI | Pending |
| Window/region capture | Permissions, resize, minimization, window closure, source switching and session expiry; never fall back to another window/display; unsupported Wayland behavior recorded | Pending (Windows generated-window resize/closure has automated native coverage only) |
| RGB recording/replay | Frame order/timing, trim/speed/loop, explicit layout remap, Stop and capture release; confirm exported RGB contains no source captures | Pending |
| Schedules, timers and app rules | Real local-time boundaries and DST; missed/sleep transitions; manual override; foreground focus and scene restoration; quiet hours and capture consent | Pending |
| Multiple controllers/layout | At least two compatible strips; stable and duplicate serials; disconnect one while the other runs; independent scene/capture/failure/recovery, route changes and group scaling | Pending |
| Handoff | Release/close before physical USB switch; other host import and explicit Start; ownership conflict and recovery; never imply software switches physical USB | Pending |
| Ambient sensors | Windows built-in sensor, Linux IIO, external lux input; unplug/stale fallback, smoothing, manual override, resume and restart | Pending |
| Counter-Strike 2 | Install fresh GSI cfg; own-player health/kills/round/bomb; spectator/menu/end/disconnect baseline; selected zone, timeout and quiet-hours restoration; disabling and update teardown | Pending |
| Declarative plugins | Sample effect and integration; enable consent, manual override, disable/remove/restart; output bounds and ordinary scene restoration on real strips | Pending |
| Phone remote | Actual Android/iOS browser certificate inspection/acceptance, private Wi-Fi/firewall/client isolation, pairing timeout/attempts, multiple clients, expiry/revoke/restart, sleep/wake/network-address changes and fixed target controller | Pending (loopback HTTPS and 390px browser controls passed automated native checks) |
| CLI/API and updater | Installed CLI PATH on each OS, capture refusal, color/off behavior, real upgrade from a prior version, installer retry and listener/USB teardown before installation | Pending (release assets, hashes and signatures verified separately) |

For accessibility, cover every new dialog, per-rule fieldset, plugin file/permission preview, pairing code and client-revocation control, controller diagram keyboard/form alternative, timer state and connection announcements with Windows and macOS screen readers. Verify focus enters/returns correctly, Escape does not silently apply drafts, keyboard-only paths exist, meaningful changes are announced without constant polling chatter, and labels/contrast/layout survive larger text and display scaling. Phone controls also need actual mobile accessibility checks. Automated DOM/CSP/layout tests do not substitute for these results.
