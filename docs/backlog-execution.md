# Remaining backlog implementation

Requested on 2026-10-19: implement all remaining features. Live GitHub issues define acceptance criteria. This document records execution order and handoffs across implementation batches.

1. Studio foundations: #36 appearance, #11 custom effects, #12 zones, #19 bias lighting, #21 local image palettes, #28 interactive preview.
2. Capture and playback: #13 window/region capture, #22 RGB sequence recording/replay.
3. Automation: #10 schedules/fades, #15 temporary accents, #25 focus timers, #9 application rules.
4. Hardware routing: #8 multiple independent controllers, #34 visual layout, #26 device handoff, #23 ambient sensors.
5. Extensions: #17 paired LAN remote, #24 supported game telemetry, #27 isolated plugins.

Each batch must include working native and UI behavior, bounded/validated persistence, meaningful regression checks, documentation, review, and release verification. Preserve existing capture consent, ordinary Stop behavior, comfort caps, and the single writer for each controller. CLI/API extensions use the versioned control protocol.

Physical validation remains tracked in #37, #38 and #45. Automated tests and CI builds do not complete physical USB, actual capture/audio, assistive technology, or cross-host testing. Record unsupported platform behavior explicitly.

Studio foundations: shipped in 0.11.0 via #56. All four platform builds passed; published 21 assets, 20 SHA256 checksums, and four production updater signatures were verified. Local validation included 69 Rust tests, strict Clippy, browser interaction/contrast/layout/CSP checks and isolated native Windows studio/Quick-controls smoke.

Capture and playback (#13, #22): implemented on `codex/capture-playback` for 0.12.0. Local validation: 75 Rust tests, strict Clippy, browser source/sequence controls, isolated native persistence/playback setup, and real Windows capture of a generated fixture window (including resize and closure). No USB was used. Cross-platform CI/release verification pending.

Capture/playback merged via #57 after all four platform builds passed. Production release run 39232125424 is in progress for v0.12.0.

Scheduling, temporary cues, focus timers and application rules (#10, #15, #25, #9): implemented on `codex/lighting-routines` for 0.13.0, with additive authenticated API/CLI controls. Local unit, strict lint, browser and isolated native Windows timer/off-state checks completed; final validation and cross-platform release pending. Details and deliberately supported platform/capture scope are in `docs/routines.md`.

Capture/playback 0.12.0 published successfully: all 21 assets, 20 checksums and four production updater signatures verified. Routine batch #58 merged after all four builds passed; 0.13.0 published with the same complete asset/hash/signature verification. A fast-click timer busy-state issue found by Linux CI was fixed before merge.

Controller routing, visual layout, handoff and ambient sensors (#8, #34, #26, #23): implemented on `codex/controller-routing` for 0.14.0. Independent workers, strict interface bindings and duplicate-owner guard; draft diagram/form routing; group scale preserves individual levels; persistent USB release and portable calibration/scene import; opt-in Windows/IIO/external lux providers. Unit, strict lint, browser routing/sensor controls and isolated native Windows fleet/group/manual-override/import smoke checks passed. Final release checks and cross-platform CI pending. See `docs/controller-routing.md` for supported scope and physical validation still required.

Next: paired LAN remote, supported game telemetry and isolated plugins (#17, #24, #27).
