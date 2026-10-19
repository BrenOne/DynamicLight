# Remaining backlog implementation

Requested on 2026-10-19: implement all remaining features. Live GitHub issues define acceptance criteria. This document records execution order and handoffs across implementation batches.

1. Studio foundations: #36 appearance, #11 custom effects, #12 zones, #19 bias lighting, #21 local image palettes, #28 interactive preview.
2. Capture and playback: #13 window/region capture, #22 RGB sequence recording/replay.
3. Automation: #10 schedules/fades, #15 temporary accents, #25 focus timers, #9 application rules.
4. Hardware routing: #8 multiple independent controllers, #34 visual layout, #26 device handoff, #23 ambient sensors.
5. Extensions: #17 paired LAN remote, #24 supported game telemetry, #27 isolated plugins.

Each batch must include working native and UI behavior, bounded/validated persistence, meaningful regression checks, documentation, review, and release verification. Preserve existing capture consent, ordinary Stop behavior, comfort caps, and the single writer for each controller. CLI/API extensions use the versioned control protocol.

Physical validation remains tracked in #37, #38 and #45. Automated tests and CI builds do not complete physical USB, actual capture/audio, assistive technology, or cross-host testing. Record unsupported platform behavior explicitly.

Studio foundations: implemented on `codex/studio-backlog` for 0.11.0. Local validation: 69 Rust tests, strict Clippy, browser interaction/contrast/layout/CSP checks, and isolated native Windows studio/Quick-controls smoke. Cross-platform CI and release verification follow before delivery.

Next: capture and playback (#13, #22).
