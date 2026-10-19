# Controllers, layout, handoff and ambient brightness

Open **Device setup → Controllers & monitor layout**. Up to eight supported ROBOBLOQ/SyncLight lighting interfaces can run simultaneously. Each has a separate lighting worker, USB handle, source capture, settings, recording, routines and sensor configuration. A disconnected strip does not stop another worker. Main, Quick controls, tray, profiles and the local CLI/API operate on the **controlled strip**, whose name appears in the main header. Choose a row and **Control this strip** to change it.

## USB bindings and independent scenes

Refresh enumerates metadata without opening devices. The existing VID/PID and lighting-interface filter still rejects keyboard interfaces. Bind the main strip explicitly before adding a second controller. A unique hardware serial is used when available; otherwise the OS HID path identifies the interface. Port changes, reinstallations or another host may require explicit rebinding. Automatic selection is accepted only when exactly one compatible interface is present. Missing saved bindings never silently select another device.

Only one worker may own a given USB interface. Release and shutdown close the HID handle before releasing the process-local ownership guard. Other vendors' control apps must be closed; DynamicLight cannot arbitrate an unrelated application's private USB protocol. The existing desktop single-instance guard prevents a second instance under the same app identity.

Each controller has its own brightness and scene memory. **Start all configured modes** explicitly starts those modes, including configured capture; it skips controllers released for handoff. **Stop all strips** stops all workers. Group brightness multiplies each individual brightness (or active sensor brightness) by the chosen percentage, preserving individual values. Calibration must finish before group brightness changes. Comfort caps still apply. Removing a secondary controller stops its worker and removes it from the active list; its saved files remain recoverable in the local application-data `controllers/` directory. The primary slot can be rebound but not removed.

## Layout preview

The canvas shows OS display geometry, explicitly mapped strips, edge coverage, LED count and the first physical LED marker. Drag a monitor to adjust this diagram, or select a monitor and use arrow keys / numeric horizontal and vertical coordinates. Diagram positions do not rearrange the OS desktop. Up to sixteen displays can appear.

Choose each strip's display, LED count, three/four edge coverage, offset and direction. **Apply mappings & routes** validates all drafts first; capture-source changes require fresh Start/Retry. Closing discards drafts. A filesystem failure during a multi-controller apply reports partial failure and requires Refresh to review actual values. The Studio button opens the selected strip's existing LED/edge/range editor, zone tools and simulated preview. All changes there remain drafts until Apply. Hardware calibration verifies the actual installation; the schematic alone does not measure LED spacing.

## Cross-host handoff

**Release controlled strip & export handoff** saves a versioned JSON file in Downloads, turns lighting off, closes USB, cancels cues/timers and pauses the controlled strip's schedules/application rules. The release state persists across restart. Group Start skips released strips. Explicit **Start** takes ownership again; schedules and app rules remain paused until explicitly re-enabled. Reconnect alone cannot take a released device back.

A physical USB switch, unplug/replug or another separately configured bridge is required. This feature does not move USB ownership over the network. On the receiving Windows, Mac or Linux host, bind the compatible controller, preview the handoff file and apply it while stopped. The file contains a portable scene and physical calibration; it excludes machine USB paths, display bindings, capture consent, comfort caps and credentials. Choose this host's display and press Start when ready. If the strip is not connected yet, normal reconnect recovery applies the chosen profile when it arrives. Capture needs fresh consent. Replay additionally needs the matching RGB sequence file. Unsupported formats/controller models and invalid calibration values are rejected.

## Ambient sensors

Sensor configuration applies to the controlled strip and is off by default. Supported sources are the [Windows LightSensor API](https://learn.microsoft.com/en-us/uwp/api/windows.devices.sensors.lightsensor.getcurrentreading), Linux [IIO illuminance channels](https://cdn.kernel.org/doc/html/latest/driver-api/iio/core.html) (`in_illuminance_input`, or raw plus scale and optional offset), and an external JSON lux input on all three platforms. macOS currently uses the external input; no private built-in sensor API is used. Discovery and reading happen outside lighting workers. Windows polling requests a driver report interval of at least one second. Unavailable sources are visible in the editor and use the configured fallback.

Lux maps linearly between chosen dark-room/bright-room values to minimum/maximum brightness. Bounds must increase and fit 0–100000 lux. Changes use exponential smoothing with a 1–120 second time constant, followed by grouped scaling, fades and existing comfort limits. Invalid/stale/disconnected sensor data uses the fixed fallback with the same smoothing. Sensor changes never start lighting. A manual brightness edit overrides automatic adjustment for the session; **Resume sensor** or saving sensor configuration resumes it. Automatic adjustment resumes on a new app session if still enabled.

An external sensor tool may atomically write a regular file at the application-data path shown in the panel:

```json
{"lux":120,"timestamp":1792400000}
```

Use the current Unix timestamp in seconds; the example timestamp is illustrative. The file must be no larger than 1 KiB, contain only these two fields, and refresh at least every thirty seconds. Future timestamps beyond five seconds and invalid readings are rejected. A missing worker update for five seconds also falls back. The app does not execute the provider, open a network connection or grant it extra privileges.

## Validation

Tests cover process-local ownership claims, unsafe slot paths, independent simulated controller failure, grouped brightness preservation, handoff release/restart/import, bounded lux mapping/smoothing/manual override/fallback and malformed external inputs. Browser checks cover diagram keyboard placement, route drafts, grouped scaling and sensor controls. Isolated native Windows checks exercise fleet state, group scaling, sensor configuration/manual override and handoff import without starting USB or capture. Real multiple strips, cross-host USB switching, OS sensor hardware, Mac/Linux desktop behavior and screen-reader checks remain tracked by #37, #38 and #45.
