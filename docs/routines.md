# Lighting routines

Open **Scenes → Schedules, timers & app rules**. Changes remain drafts until **Save routines**. Closing discards unsaved changes. Configuration is local to this installation in `routines.json`; it is not included in portable scene exports. Saving resets the running focus timer. The desktop app must remain running.

## Schedules

Choose a saved offline Color, Effects or Replay scene, local time, and weekdays. Each row stores a copy of the selected scene and its local display binding; re-add the row after editing its source profile. Up to 32 rows are supported. The first matching row wins when times overlap. Disable or remove individual rows, move rows up to change precedence, or pause all schedules.

Immediate applies and starts the selected scene. Fade-in starts that scene at reduced brightness and ramps toward its configured brightness. Fade-out dims the currently running scene and then turns lighting off; it does not apply the row's selected scene. Fades last 1–3600 seconds and retain global brightness/transition limits. Manual lighting changes cancel a fade. A later scheduled action can turn lighting back on: pause schedules to prevent this.

The scheduler follows the operating system's local timezone. It runs only on an observed consecutive minute boundary. It skips startup/current-minute configuration changes, missed events, suspended/locked display periods, backward jumps, forward jumps and the minute in which the UTC offset changes. A persistent per-row local-date ledger prevents a repeated daylight-saving hour or restart from running the same row twice on that date. Missing spring-forward times are skipped. No catch-up events run on wake. The next scheduled action includes its local UTC offset. Schedules do not start screen/audio capture or prompt for permissions; capture scenes must be started manually.

## Temporary cues and timers

Enable each wanted source separately: the manual/CLI cue, focus/completion boundaries, and break boundaries. Each source has its own RGB color and duration of 1–10 seconds. The cue overlays generated LED colors before comfort processing. The underlying scene continues, and its settings and power intent remain unchanged. One pending cue is retained; the newest event replaces an older pending event. Stop, calibration, sleep and quiet hours clear pending cues. Stopped lights remain off. Notification text, screenshots and audio content are never read by this feature.

Quiet hours may cross midnight; equal endpoints mean quiet all day. They suppress cues, including timer boundaries, but do not pause schedules or the timer itself.

Focus duration is 1–240 minutes, break duration 1–120 minutes, with 1–20 cycles. Each cycle includes focus and break. Start, pause/resume, skip phase and cancel are available. The focus cue also signals completion. Timers use monotonic elapsed time, pause on system sleep or a worker interruption longer than five seconds, and never catch up through missed phases. They do not persist across app restarts. Their cues never enable lighting or capture.

## Application rules

Refresh the application list and explicitly choose an app and offline scene. Enable application rules, then Save. Only app names are retained; window titles and application-usage history are not stored or uploaded. Matching uses exact OS-reported application names, so similarly named apps are not a security boundary. First matching row wins; Move up changes precedence. The foreground app is checked once per second on a separate worker; this does not capture pixels.

Rules only apply while lighting is already requested on. Focus loss or process exit restores the previous scene. Manual scene, brightness, power or calibration changes atomically discard the pending restoration and suppress the current rule until application focus changes. **Resume rules for current application** explicitly permits it again. A previous capture scene restores stopped and requires fresh Start consent. Failures are shown in routine status. Windows and macOS use native foreground information through xcap; macOS can require OS permissions to enumerate apps. X11 is supported; Wayland does not expose this information and reports an unavailable status.

## CLI / API v1

With Local automation enabled, the following additive commands use its existing credential, revocation, rate limits and loopback-only listener:

```sh
dynamiclight routines status --json
dynamiclight routines start
dynamiclight routines pause
dynamiclight routines skip
dynamiclight routines cancel
dynamiclight routines cue
dynamiclight routines pause-schedules
dynamiclight routines resume-schedules
dynamiclight routines resume-rules
```

`pause` toggles timer pause/resume; do not replay it automatically after a network error. `cue` uses the explicitly enabled manual cue source. `start` starts only the timer, never lighting or capture. These controls return routine status; desktop UI owns configuration. API command: `{"command":"routine","action":"status"}` with `action` one of `status`, `start`, `pause`, `skip`, `cancel`, `cue`, `pauseSchedules`, `resumeSchedules`, `resumeRules`. Earlier desktop versions reject these new commands; all earlier v1 commands remain compatible.

Automated checks cover bounded/coalesced cues, quiet hours/off, timer gaps and phases, schedule date deduplication across reload, clock/timezone discontinuities, app-rule restore/manual override, comfort caps, CLI parsing and browser/native controls. Actual macOS/X11 foreground behavior, hardware fades and assistive technology validation remain in #37, #38 and #45.
