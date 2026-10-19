# Games and plugins

First published in 0.16.0 together with the phone remote. The planned 0.15.0 release was withheld after final review found a Solid-mode extension output bug; packet-level regression coverage and its correction are included in 0.16.0.

Open **Device → Games & plugins** and choose a controller. Start ordinary lighting before activating an effect. Extensions share that controller's existing single writer and comfort processing; they never start USB, screen or audio capture. Manual scene/brightness changes, Stop, calibration and suspension cancel active effect overlays. Disable or remove revokes active output and restores the ordinary scene. Effects are temporary and do not rewrite profiles.

## Counter-Strike 2

The supported interface is Valve's [Game State Integration](https://developer.valvesoftware.com/wiki/Counter-Strike:_Global_Offensive_Game_State_Integration), used by Counter-Strike 2. No injection, memory reading, game hooks or process modifications are used. Real game/hardware validation remains in QA-01 and PLAT-01.

1. Choose damage, kill, round-start and/or bomb-planted events. Set colors, 1–10 second durations and normalized zones (0–1000) along the calibrated LED route. Up to eight mappings are accepted. Later matching rules win; repeated events replace the current cue instead of accumulating a queue.
2. Enable telemetry, then export setup. Copy `gamestate_integration_dynamiclight.cfg` from Downloads into the game's `Counter-Strike Global Offensive/game/csgo/cfg` directory and restart the game.
3. Each enable generates a fresh loopback port and random 256-bit authentication credential. Export and replace setup after re-enabling. App restart leaves telemetry disabled. Treat the setup file as a local credential; do not share it. Disable revokes it immediately. Remove the cfg to stop game posts.

The listener accepts authenticated JSON only at its exact loopback Host/path, rejects browser Origins, duplicate headers and chunked bodies, bounds headers to 8 KiB and bodies to 32 KiB, applies a two-second read deadline, and handles one connection at a time at 10 requests/second (burst 20). It does not expose an API response containing capture or game data. Payloads are discarded after extracting bounded health, round kills and round/bomb phases; player identity is compared only to reject spectator events. Names, raw payloads and identity are not retained. Invalid credentials cannot extend the active session. The first frame establishes a baseline. Menu/game-over/spectating and ten seconds without valid active-player telemetry revoke cues. Quiet hours, off state, suspension and calibration suppress them; the underlying scene keeps running and reappears after the cue.

## Declarative plugin API v1

This initial plugin API supports **data-only programs**: bounded compositions for effects, or event-to-cue rules for integrations. It does not run third-party JavaScript, WASM, native code, processes or host callbacks. Unsupported capabilities and unknown manifest fields are rejected. This isolation is achieved by restricting the language rather than executing untrusted code in the renderer.

Examples: [`ocean-wave.json`](../examples/plugins/ocean-wave.json) and [`game-hit.json`](../examples/plugins/game-hit.json).

Manifest fields:

| Field | Contract |
| --- | --- |
| `apiVersion` | Exactly `1`; incompatible versions are rejected |
| `id` | Unique 1–64 ASCII letters, digits or hyphens |
| `name`, `version` | Display name up to 100 bytes; version up to 30 ASCII alphanumeric, dot or hyphen characters |
| `capabilities` | Effect: exactly `["lighting"]`; integration: exactly `["lighting", "gameEvents"]` |
| `program.kind` | `effect` or `integration` |
| `program.composition` | Effect only: existing Studio composition v1; 1–4 layers, 2–16 color stops/layer, validated duration/repeats |
| `program.rules` | Integration only: 1–8 `{event,color,seconds,start,end}` mappings as above |

Lifecycle: preview → install disabled → enable → activate effect / receive opted-in telemetry → disable or remove. Install validates the entire manifest before atomic persistence. Up to 16 plugins, 32 KiB per imported manifest, and 512 KiB registry on load. Duplicate IDs require explicit removal before replacement. All plugins start disabled after application restart. Integration enable additionally requires the visible game-event consent checkbox; the receiver must also be explicitly enabled. Only event names from validated numeric game telemetry reach integration programs, never raw payloads. Disable/remove revokes queued and active output. At most one effect and one event cue are active per controller; later activations replace earlier ones. Rules from enabled integration plugins follow built-in mappings in installation order.

There are no filesystem, network, screen/audio, USB, renderer or arbitrary-command capabilities in v1. Resource use is bounded by manifest size, layer/stop/rule limits, the existing 127-LED maximum and frame rate. Bad imports leave the registry and active scene unchanged. Ordinary profiles remain usable if an extension is absent or disabled. Editing an effect permanently remains available through Studio; plugin activation is intentionally temporary.

Automated validation covers schema limits and rejected capabilities/code, event filtering, HTTP ambiguity, cue revocation, scene restoration, off/suspend/comfort behavior, browser installation/enable/disable/remove and isolated native IPC. These checks do not replace live game, physical lighting or screen-reader testing.
