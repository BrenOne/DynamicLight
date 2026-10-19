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

Next: scheduling, temporary cues, focus timers and application rules (#10, #15, #25, #9).
