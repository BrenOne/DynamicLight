# Backlog execution and validation handoff

Requested on 2026-10-19: implement the remaining backlog all the way through release. All 36 implementation features are now represented in the 0.16 source tree. No feature implementation remains in the backlog. Physical validation is separately owned by #37, #38 and #45; it has not been claimed complete.

| Batch | Pull request | Scope |
| --- | --- | --- |
| Studio, 0.11 | #56 | Appearance, custom compositions, zones, bias calibration, local palettes and interactive preview |
| Capture/playback, 0.12 | #57 | Window/region capture and bounded RGB recording/replay |
| Routines, 0.13 | #58 | Schedules/fades, app rules, cues and focus timers; additive CLI/API controls |
| Hardware, 0.14 | #59 | Independent controllers, monitor/strip routing, group scale, USB handoff and ambient sensors |
| Extensions, included in 0.16 | #60 and #61 | Counter-Strike 2 GSI and bounded declarative effect/integration plugins; physical-frame output correction |
| Phone remote, 0.16 | #61 | Opt-in HTTPS, expiring pairing, client revocation, constrained controls and shared CLI color command |

Releases 0.11–0.14 were published and independently checked: 21 assets each, 20 checksums, four production updater signatures and the Windows CLI version. The planned 0.15 release was cancelled before publication after final review found that extension zones over Solid scenes could send only the first pixel through the USB fast path. Version 0.16 includes the correction and packet-level regression for mapped pixels and immediate restoration; games/plugins first ship with the phone remote.

The final local validation includes 99 passing native tests (one separate interactive Windows capture fixture skipped in the normal suite), nine control/CLI tests, strict Clippy/formatting, browser desktop workflows, 14 release tests, an actual TLS listener/client roundtrip and isolated native Windows phone/CLI smoke. The HTTPS smoke verifies certificate/IP, Origin/auth refusal, one-use pairing, revocation, responsive phone color/brightness/Stop and preservation of off state. Test listeners bind loopback only; no firewall, physical USB or capture changes were made. The Windows generated-window capture resize/closure fixture was run separately during the capture batch.

Each release still requires all four platform builds and the production publication job. The release scripts validate a complete asset set, exact version/tag, checksums and updater signatures before publishing. Published assets are immutable; CLI and desktop artifact names remain distinct. Main merges with a new manifest version trigger publication automatically.

See [validation through 0.16](backlog-validation.md) for the complete pending physical handoff, and the feature guides: [routines](routines.md), [controllers](controller-routing.md), [extensions](extensions.md), [phone remote](phone-remote.md). Real Mac/Windows/Linux hardware, mobile certificate acceptance, private-network/firewall behavior and screen readers need recorded results in the open validation trackers.
