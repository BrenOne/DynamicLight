# Local automation and CLI (0.10+)

Keep the desktop app running and enable **Settings → Local automation → Enable local API and CLI controls**. It is off by default. Closing the window to the tray keeps automation available; quitting stops it. The CLI is a companion to the desktop engine, not a separate USB controller or service.

## Install

Download `install.sh` or `install.ps1` from the [latest release](https://github.com/mskutin/DynamicLight/releases/latest), following the [terminal installation guide](installation.md). Then:

```sh
bash install.sh --cli
export PATH="$HOME/.local/bin:$PATH"
dynamiclight status
```

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\install.ps1 -Cli
$env:PATH = "$env:LOCALAPPDATA\DynamicLightCLI\bin;$env:PATH"
dynamiclight status
```

These PATH changes apply to the current terminal. Add the directory to your shell profile or Windows user PATH for other terminals. Installers never edit PATH or start the app. Version pinning and download-only options also work for CLI downloads. Separate `dynamiclight-cli-*` binaries support Windows x64, macOS Apple silicon/Intel (13+), and Linux x64 (Ubuntu 24.04+). Manual Unix downloads need `chmod +x` and can be renamed `dynamiclight`.

Rerun the CLI installer to update it. Desktop in-app updates update the desktop package only. API v1 clients remain compatible with desktop releases supporting v1. Remove the CLI binary to uninstall it; disable Local automation to revoke access.

## Commands and scripts

Version 0.13 adds [routine controls](routines.md#cli--api-v1) for timers, cues and schedule overrides through this same authenticated API.

Version 0.16 adds `dynamiclight color '#336699'` / `{"version":1,"command":{"command":"color","value":"#336699"}}`. This selects Solid mode and preserves stopped state. The separately paired [HTTPS phone remote](phone-remote.md) reuses a restricted subset of API v1; its cookies cannot authenticate the local CLI API.

```sh
dynamiclight status --json
dynamiclight power on
dynamiclight brightness 35
dynamiclight scenes list
dynamiclight scenes apply warm-desk
dynamiclight profiles list
dynamiclight profiles apply my-profile-id
dynamiclight power toggle
dynamiclight power off
```

Use IDs returned by `list`, not display names. Scene/profile application starts lighting; brightness changes the saved setting without turning lights on. Power on starts the current mode. Toggle uses requested power, including when lifecycle rules have temporarily paused output. Status reports requested and actual power, connection, mode, requested brightness, capture state, pause reason, and engine error. It does not return frames, audio samples, source identifiers, or credentials. Success means the command was accepted; check `connected`, `powered`, and `error` for hardware/capture failures or recovery. Maximum brightness, transition limits, sleep/lock rules, source bindings, and calibration rules still apply.

A shell script can select a scene and adjust it:

```sh
#!/usr/bin/env sh
set -eu
dynamiclight scenes apply warm-desk
dynamiclight brightness 25
```

Exit codes: 0 accepted, 1 connection/API failure, 2 invalid arguments. `--json` prints the versioned response on stdout; errors also go to stderr. `--connection FILE` selects a discovery file for an isolated installation. The CLI always connects to `127.0.0.1`. Help/version work without the desktop app.

### Capture and revocation

Screen, Music, and Blend require both **Allow automation to start screen and audio capture** in Settings and explicit consent on each start command:

```sh
dynamiclight scenes apply cinema --allow-capture
dynamiclight power on --allow-capture
```

Grant system capture permissions through the desktop app first. Automation preflights permissions without requesting a new OS prompt. Capture commands without consent fail before changing mode or starting hardware. Brightness, status, and power off do not need capture consent.

Changing either automation setting rotates credentials; **Rotate credential** also revokes the old session. Disabling or quitting removes the connection file. Already executing commands may finish; queued engine commands check revocation before applying changes. Desktop updates stop automation before installation and create a fresh session on restart. After an installer failure, restart the app or re-enable automation.

## HTTP API v1

The listener binds only to a random `127.0.0.1` port. The private discovery file contains `version`, `port`, and a random 256-bit `token`. Settings shows its path:

| OS | Default connection file |
| --- | --- |
| Windows | `%APPDATA%\com.dynamiclight.desktop\automation\session.json` |
| macOS | `~/Library/Application Support/com.dynamiclight.desktop/automation/session.json` |
| Linux | `${XDG_DATA_HOME:-$HOME/.local/share}/com.dynamiclight.desktop/automation/session.json` |

Unix permissions are 0700 on the directory and 0600 on the file. Windows uses a protected ACL for the owner and LocalSystem. Do not share this file, log its token, or commit it. Applications running as your OS user can read the credential and control the app; this feature does not isolate software running under the same account.

Send `POST /v1/command`, `Content-Type: application/json`, `Authorization: Bearer TOKEN`, and `Host: 127.0.0.1:PORT`:

```json
{"version":1,"command":{"command":"brightness","value":35}}
```

| Command object | Meaning |
| --- | --- |
| `{"command":"status"}` | Read status |
| `{"command":"power","state":"on","allowCapture":false}` | on, off, or toggle |
| `{"command":"brightness","value":35}` | Integer 0–100 |
| `{"command":"scenes"}` / `{"command":"profiles"}` | List IDs, names, modes |
| `{"command":"applyScene","id":"warm-desk","allowCapture":false}` | Apply built-in scene |
| `{"command":"applyProfile","id":"my-profile-id","allowCapture":false}` | Apply saved profile |

`allowCapture` defaults to false. Unknown fields, commands, versions, and invalid ranges fail. Responses are `{"version":1,"ok":true,"data":...}` or `{"version":1,"ok":false,"error":"..."}`. HTTP codes: 200 accepted, 400 malformed, 401 invalid/revoked credential, 403 browser/nonlocal host, 404 wrong method/path, 409 engine rejection, 413 oversized body, 415 wrong content type, 429 rate limit. Error text is for people, not a stable machine identifier.

Headers and bodies are each limited to 8 KiB, with a two-second absolute read deadline. A session-wide bucket allows 10 requests/second and bursts of 20, including failed authentication. Connections are sequential and close after one response. Browser Origin headers, CORS, transfer encoding, and duplicate headers are rejected. Do not expose/proxy this endpoint to a network. Reread the file after restart/rotation and back off on failures; never automatically replay a toggle.

PowerShell example reads credentials locally without putting the token in process arguments:

```powershell
$file = Join-Path $env:APPDATA 'com.dynamiclight.desktop\automation\session.json'
$connection = Get-Content -LiteralPath $file -Raw | ConvertFrom-Json
$body = @{version=1; command=@{command='status'}} | ConvertTo-Json -Compress
Invoke-RestMethod -Method Post -Uri "http://127.0.0.1:$($connection.port)/v1/command" `
  -Headers @{Authorization="Bearer $($connection.token)"} -ContentType 'application/json' -Body $body
```

## Stream Deck and home automation

Create a Stream Deck Open action targeting a local script. On Windows, a `.cmd` file can contain:

```bat
@echo off
"%LOCALAPPDATA%\DynamicLightCLI\bin\dynamiclight.exe" scenes apply warm-desk
```

On macOS, use an executable `.command` file with `#!/bin/sh` followed by `"$HOME/.local/bin/dynamiclight" power toggle`. Make separate buttons for off, brightness levels, or profile IDs. No token needs to be stored in Stream Deck. Capture buttons must deliberately include `--allow-capture` after enabling the Settings permission.

A home-automation adapter should run on the same computer and user account as DynamicLight. Invoke the CLI with `--json` or read the discovery file and call v1, exposing only selected commands through the adapter's authenticated integration. Prefer idempotent on/off commands, poll status at about one second, and back off on 429 or desktop absence. A remote Home Assistant host needs a separately secured local bridge; there is no LAN listener or remote pairing here. Phone remote (#17) and game telemetry (#24) remain separate backlog features.
