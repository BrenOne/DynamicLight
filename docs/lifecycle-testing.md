# Startup and recovery validation

Automated tests cover intent persistence, opt-in startup/capture rules, overlapping pause reasons, Stop during suspend, changed-source consent, UI recovery actions and login-registration rollback. Windows native smoke checks load the real WebView/IPC and register OS watchers without USB or capture. CI builds Windows, both Mac architectures, and Linux; Linux also exercises X11 capture and virtual system audio.

Physical checks below remain pending. A successful build or synthetic event is not a physical sleep/wake test.

Record physical results and defects in [Windows/Linux validation #45](https://github.com/mskutin/DynamicLight/issues/45) or [macOS validation #38](https://github.com/mskutin/DynamicLight/issues/38). Track keyboard and assistive-technology findings in [accessibility #37](https://github.com/mskutin/DynamicLight/issues/37). Closing a delivered feature does not mark these checks passed.

| Scenario | Expected | Windows | Mac | Linux |
| --- | --- | --- | --- | --- |
| Fresh install / upgrade | No login registration or capture without opt-in | Pending | Pending | Pending |
| Login startup enabled; restore disabled | App starts in tray (Linux minimized), strip remains off | Pending | Pending | Pending |
| Restore enabled; previous mode running | Color/effects resume; screen/music require capture-resume opt-in | Pending | Pending | Pending |
| Stop, quit, relaunch | Remains off even with restore enabled | Pending | Pending | Pending |
| USB unplug/replug; busy device released | Retry at three-second intervals, recover same mode; Stop cancels | Pending | Pending | Pending |
| Lock → display off → suspend → wake → unlock | No premature restart while another configured pause remains | Pending | Pending | Pending |
| Display-only sleep | Lights off/on according to preference without changing intended power | Pending | Pending | Pending (X11 only) |
| Stop while paused | No restart after wake/unlock | Pending | Pending | Pending |
| Sleep blackout disabled | Last frame remains if USB stays powered; capture releases | Pending | Pending | Pending |
| Default speaker / microphone switch | Capture follows new default when automatic capture resume is enabled | Pending | Pending | Pending |
| Selected monitor removed, reattached or resized | Pause/retry selected source; never silently capture a different monitor | Pending | Pending | Pending |
| Capture permission denied/revoked | Clear error; automatic recovery does not repeatedly request permission | Pending | Pending | Pending |
| Second app instance | Shows existing app; no competing USB writer | Pending | Pending | Pending |
| Login entry disabled in OS settings | UI reflects registration when reopened; no automatic re-enable | Pending | Pending | Pending |

Mac lock notifications use best-effort platform behavior and require validation on supported macOS versions. Linux lock detection depends on the desktop publishing logind LockedHint. Wayland display-only sleep is unsupported and reported in the UI.
