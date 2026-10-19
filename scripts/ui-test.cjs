// Browser-only UI regression tests. The IPC fixture never accesses USB or audio.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const http = require("node:http");
const { chromium } = require(process.env.PLAYWRIGHT_PATH || "playwright");
(async () => {
  const root = path.resolve("src/ui");
  const server = http.createServer((req, res) => {
    const name = req.url === "/" ? "index.html" : req.url.slice(1);
    if (
      ![
        "index.html",
        "style.css",
        "renderer.js",
        "updates.js",
        "automation.js",
        "appearance.js",
        "studio.js",
        "image-palette.js",
        "capture-sources.js",
        "sequences.js",
        "routines.js",
        "controllers.js",
        "ambient.js",
        "extensions.js",
        "remote.js",
        "library-tools.js",
        "ui-common.js",
        "icons.svg",
        "icon.svg",
        "compact.html",
        "compact.css",
        "compact.js",
      ].includes(name)
    ) {
      res.writeHead(404).end();
      return;
    }
    res.setHeader(
      "Content-Type",
      name.endsWith(".svg")
        ? "image/svg+xml"
        : name.endsWith(".css")
          ? "text/css"
          : name.endsWith(".js")
            ? "text/javascript"
            : "text/html",
    );
    // Match the application's CSP, including the absence of unsafe-inline.
    res.setHeader(
      "Content-Security-Policy",
      "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; object-src 'none'; frame-src 'none'",
    );
    res.end(fs.readFileSync(path.join(root, name)));
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  let browser;
  try {
    browser = await chromium.launch({
      headless: true,
      ...(process.env.UI_BROWSER_CHANNEL
        ? { channel: process.env.UI_BROWSER_CHANNEL }
        : {}),
    });
    const page = await browser.newPage({
      viewport: { width: 1280, height: 1000 },
      deviceScaleFactor: 1,
    });
    const errors = [];
    page.on("pageerror", (e) => errors.push(e.message));
    await page.addInitScript((appVersion) => {
      const initial = {
        color: "#9b72ff",
        brightness: 65,
        count: 65,
        mode: "solid",
        sides: 3,
        reverse: false,
        offset: 0,
        smoothing: 65,
        audioSource: "system",
        sensitivity: 50,
        fps: 30,
        screenId: "",
        captureSource: { kind: "display" },
        offOnSleep: true,
        offOnDisplaySleep: true,
        offOnLock: true,
        restoreOnStart: false,
        resumeCapture: false,
        offOnQuit: true,
        effect: "aurora",
        palette: "aurora",
        speed: 35,
        customPalette: [],
        musicStyle: "spectrum",
        noiseFloor: 0,
        release: 40,
        excludeBars: false,
        rgbBalance: [100, 100, 100],
        saturation: 100,
        gamma: 100,
        inputGain: 0,
        blend: 60,
        maxBrightness: 100,
        transitionLimit: 0,
        minimumGlow: 0,
      };
      const state = {
        settings: initial,
        status: {
          connected: false,
          message: "Connect your USB strip to get started.",
        },
        powered: false,
        requested: false,
        pauseReason: "",
        recovering: false,
        captureRequiresStart: false,
        lifecycleWarning: "",
        capturing: false,
        colors: [],
        fps: 0,
        error: "",
        captureSize: [0, 0],
        audioSamples: 0,
        framesWritten: 0,
        audioBands: Array(32).fill(0),
        audioLevel: 0,
        lastWriteMs: null,
        writeDurationMs: 0,
        writeErrors: 0,
      };
      const calls = [];
      const listeners = {};
      let listener;
      let calibrationOriginal;
      let profileLibrary = { profiles: [] };
      let controllerPositions = {};
      const controllerView = () => ({
        selected: "primary",
        positions: structuredClone(controllerPositions),
        controllers: [
          { key: "primary", name: "Main strip", state: structuredClone(state) },
        ],
      });
      const ambientState = () =>
        (state.ambient ||= {
          config: {
            enabled: false,
            source: "external",
            lowLux: 0,
            highLux: 500,
            minimum: 10,
            maximum: 60,
            fallback: 35,
            smoothing: 5,
          },
          lux: null,
          available: false,
          manualOverride: false,
          brightness: 35,
          message: "No sensor reading; fixed fallback",
        });
      let routineConfig = {
        paused: true,
        schedules: [],
        appRules: [],
        appsEnabled: false,
        quietEnabled: false,
        quietStart: 1320,
        quietEnd: 480,
        focusMinutes: 25,
        breakMinutes: 5,
        cycles: 1,
        manualCue: { enabled: false, color: "#86b7ff", seconds: 3 },
        focusCue: { enabled: false, color: "#86b7ff", seconds: 3 },
        breakCue: { enabled: false, color: "#86b7ff", seconds: 3 },
      };
      function portableProfile(p) {
        const settings = structuredClone(p.settings);
        const source = p.source || { screenId: settings.screenId || "" };
        for (const key of [
          "count",
          "sides",
          "reverse",
          "offset",
          "screenId",
          "captureSource",
          "offOnSleep",
          "offOnDisplaySleep",
          "offOnLock",
          "offOnQuit",
          "restoreOnStart",
          "resumeCapture",
          "maxBrightness",
          "transitionLimit",
          "minimumGlow",
          "modeSettings",
        ])
          delete settings[key];
        return { ...p, settings, source };
      }
      function colors() {
        state.colors = Array.from({ length: state.settings.count }, (_, i) =>
          state.powered
            ? [
                Math.round(65 + (100 * (1 + Math.sin(i / 8))) / 2),
                Math.round(90 + (110 * (1 + Math.cos(i / 9))) / 2),
                170,
              ]
            : [0, 0, 0],
        );
      }
      window.fixture = {
        update: {
          phase: "idle",
          currentVersion: appVersion,
          version: null,
          notes: "",
          message: "",
          automatic: true,
          canInstall: true,
          progress: null,
        },
        updateInstalls: 0,
        state,
        calls,
        emitEvent: (name, payload) => listeners[name]?.({ payload }),
        emit: () => {
          colors();
          listener?.({ payload: structuredClone(state) });
        },
      };
      window.__TAURI__ = {
        event: {
          listen: async (name, callback) => {
            listeners[name] = callback;
            if (name === "lighting") listener = callback;
          },
        },
        core: {
          invoke: async (cmd, args) => {
            if (cmd === "remote_manage") {
              fixture.remote ||= {
                enabled: false,
                interfaces: [{ address: "127.0.0.1", name: "Local test" }],
                clients: [],
              };
              if (args.operation === "enable")
                Object.assign(fixture.remote, {
                  enabled: true,
                  controller: args.controller,
                  endpoint: "https://127.0.0.1:1234",
                  fingerprint: "AA:BB",
                  pairingCode: "123456",
                  pairingSeconds: 120,
                });
              if (args.operation === "disable")
                Object.assign(fixture.remote, {
                  enabled: false,
                  clients: [],
                  pairingCode: null,
                });
              if (args.operation === "revokeAll") {
                fixture.remote.clients = [];
                fixture.remote.pairingCode = null;
              }
              if (args.operation === "pair")
                fixture.remote.pairingCode = "654321";
              return structuredClone(fixture.remote);
            }
            if (cmd === "game_status")
              return fixture.game || { enabled: false, connected: false };
            if (cmd === "game_configure") {
              fixture.game = {
                enabled: !!args.config,
                connected: false,
                config: args.config,
                endpoint: "http://127.0.0.1:2345/gsi",
              };
              return fixture.game;
            }
            if (cmd === "plugin_manage") {
              fixture.plugins ||= [];
              if (args.operation === "preview") return JSON.parse(args.text);
              if (args.operation === "install")
                fixture.plugins.push({
                  manifest: JSON.parse(args.text),
                  enabled: false,
                });
              if (args.operation === "enable" || args.operation === "disable")
                fixture.plugins.find((p) => p.manifest.id === args.id).enabled =
                  args.operation === "enable";
              if (args.operation === "remove")
                fixture.plugins = fixture.plugins.filter(
                  (p) => p.manifest.id !== args.id,
                );
              return structuredClone(fixture.plugins);
            }
            if (cmd === "controller_list") return controllerView();
            if (cmd === "controller_devices")
              return [{ id: "p-01", name: "Fixture strip", serial: false }];
            if (cmd === "controller_manage") return controllerView();
            if (cmd === "controller_routes") {
              for (const r of args.routes) {
                const { key, ...patch } = r;
                Object.assign(state.settings, patch);
              }
              controllerPositions = structuredClone(args.positions);
              return controllerView();
            }
            if (cmd === "controller_group") {
              if (args.operation === "brightness")
                state.groupBrightness = args.value;
              if (args.operation === "stop")
                state.powered = state.requested = false;
              return controllerView();
            }
            if (cmd === "ambient_config") {
              const s = ambientState();
              if (args.config) s.config = structuredClone(args.config);
              return structuredClone(s);
            }
            if (cmd === "ambient_sources")
              return {
                sources: [
                  {
                    id: "external",
                    name: "Fixture external sensor",
                    available: false,
                    message: "No sample",
                  },
                ],
                externalFile: "/fixture/ambient-input.json",
              };
            if (cmd === "routines_config") {
              if (args.config) routineConfig = structuredClone(args.config);
              return structuredClone(routineConfig);
            }
            if (cmd === "routine_apps")
              return ["Fixture editor", "Fixture video"];
            if (cmd === "import_sequence") {
              const clip = JSON.parse(args.text);
              state.sequence = {
                recording: false,
                clip: {
                  id: clip.id,
                  count: clip.count,
                  sides: clip.sides,
                  durationMs: clip.durationMs,
                  frames: clip.frames.length,
                },
              };
              return structuredClone(state);
            }
            if (cmd === "export_sequence")
              return "Downloads/DynamicLight-rgb-test.json";
            if (cmd === "capture_windows_list")
              return [
                {
                  name: "Test player",
                  source: {
                    kind: "window",
                    id: 123,
                    pid: 456,
                    app: "Player",
                    session: "test",
                  },
                },
              ];
            if (cmd === "studio_library") {
              if (args.next) fixture.studioLibrary = args.next;
              return structuredClone(
                fixture.studioLibrary || { version: 1, effects: [], bias: [] },
              );
            }
            if (cmd === "studio_preview")
              return Array.from({ length: args.settings.count }, () => [
                40, 80, 120,
              ]);
            if (cmd === "appearance") {
              if (args.value) fixture.appearance = args.value;
              return structuredClone(
                fixture.appearance || {
                  theme: "system",
                  accent: "violet",
                  density: "spacious",
                },
              );
            }
            if (cmd === "automation_status")
              return structuredClone(
                fixture.automation || {
                  enabled: false,
                  allowCapture: false,
                  endpoint: null,
                  connectionFile: "private/session.json",
                  message: "",
                },
              );
            if (cmd === "automation_configure") {
              if (fixture.failAutomation)
                throw Error("Automation settings could not be saved");
              fixture.automation = {
                ...args,
                endpoint: args.enabled
                  ? "http://127.0.0.1:12345/v1/command"
                  : null,
                connectionFile: "private/session.json",
                message: "",
              };
              fixture.automationChanges = (fixture.automationChanges || 0) + 1;
              return structuredClone(fixture.automation);
            }
            if (cmd === "update_status") return structuredClone(fixture.update);
            if (cmd === "update_check") {
              fixture.update.phase = fixture.update.version
                ? "available"
                : "current";
              return structuredClone(fixture.update);
            }
            if (cmd === "update_automatic") {
              if (fixture.failUpdatePreference)
                throw Error("Update settings could not be saved");
              fixture.update.automatic = args.enabled;
              return structuredClone(fixture.update);
            }
            if (cmd === "update_install") {
              fixture.updateInstalls++;
              fixture.update.phase = "error";
              fixture.update.message =
                "Download or signature verification failed. Lighting has not been stopped.";
              return structuredClone(fixture.update);
            }
            if (cmd === "effect_previews") {
              window.fixture.previewCalls =
                (window.fixture.previewCalls || 0) + 1;
              return Array.from({ length: 5 }, (_, e) =>
                Array.from({ length: 48 }, (_, i) => [
                  60 + e * 35,
                  120 + Math.round(70 * Math.sin(i / 8 + args.seconds)),
                  190,
                ]),
              );
            }
            if (cmd === "profile_library") {
              if (args.library)
                profileLibrary = {
                  ...structuredClone(args.library),
                  profiles: args.library.profiles.map(portableProfile),
                };
              return structuredClone(profileLibrary);
            }
            if (cmd === "remember_color") {
              profileLibrary.recent_colors = [
                args.color,
                ...(profileLibrary.recent_colors || []).filter(
                  (c) => c !== args.color,
                ),
              ].slice(0, 12);
              return structuredClone(profileLibrary);
            }
            if (cmd === "export_profiles") {
              fixture.lastExport = args;
              return "Downloads/DynamicLight-scenes-test.json";
            }
            if (cmd === "preview_profile_import") {
              const bundle = JSON.parse(args.text);
              if (bundle.version !== 1 || bundle.format !== "dynamiclight")
                throw Error("Unsupported scene file");
              return {
                bundle,
                base: structuredClone(profileLibrary),
                conflicts: bundle.profiles.map(
                  (p) =>
                    profileLibrary.profiles.find(
                      (o) => o.id === p.id || o.name === p.name,
                    )?.name || null,
                ),
                palette_conflicts: bundle.palettes.map(
                  (p) =>
                    profileLibrary.palettes?.find(
                      (o) => o.id === p.id || o.name === p.name,
                    )?.name || null,
                ),
              };
            }
            if (cmd === "import_profiles") {
              if (
                JSON.stringify(args.expected) !== JSON.stringify(profileLibrary)
              )
                throw Error("Your library changed. Choose the file again.");
              for (const p of args.bundle.profiles) {
                const i = profileLibrary.profiles.findIndex(
                  (o) => o.id === p.id || o.name === p.name,
                );
                if (i >= 0 && args.policy === "skip") continue;
                if (i >= 0 && args.policy === "replace")
                  profileLibrary.profiles[i] = {
                    ...profileLibrary.profiles[i],
                    settings: p.settings,
                  };
                else
                  profileLibrary.profiles.push({
                    ...p,
                    id: crypto.randomUUID(),
                    name: p.name + (i >= 0 ? " (2)" : ""),
                    favorite: false,
                    shortcut: "",
                    source: { screenId: "" },
                  });
              }
              return structuredClone(profileLibrary);
            }
            if (cmd === "starter_scenes")
              return {
                profiles: [
                  {
                    id: "warm-desk",
                    name: "Warm desk",
                    favorite: false,
                    shortcut: "",
                    settings: {
                      ...initial,
                      mode: "solid",
                      brightness: 28,
                      color: "#ffbf80",
                    },
                  },
                  {
                    id: "cinema",
                    name: "Cinema",
                    favorite: false,
                    shortcut: "",
                    settings: { ...initial, mode: "screen", brightness: 35 },
                  },
                  {
                    id: "neon",
                    name: "Neon",
                    favorite: false,
                    shortcut: "",
                    settings: {
                      ...initial,
                      mode: "rainbow",
                      effect: "aurora",
                      brightness: 55,
                    },
                  },
                  {
                    id: "ocean",
                    name: "Ocean",
                    favorite: false,
                    shortcut: "",
                    settings: {
                      ...initial,
                      mode: "rainbow",
                      effect: "aurora",
                      palette: "ocean",
                      brightness: 40,
                    },
                  },
                ].map(portableProfile),
              };
            if (cmd === "scene_previews") {
              fixture.sceneCalls = (fixture.sceneCalls || 0) + 1;
              if (fixture.sceneDelay)
                await new Promise((r) => setTimeout(r, fixture.sceneDelay));
              return args.settings.map((s) =>
                Array.from({ length: 48 }, (_, i) => [50 + i * 3, 140, 200]),
              );
            }
            if (cmd === "apply_starter") {
              state.settings = {
                ...initial,
                mode: "rainbow",
                palette: "ocean",
              };
              state.powered = true;
              state.requested = true;
              return structuredClone(state);
            }
            if (cmd === "compact_window") {
              if (args.operation === "pin") fixture.pinned = args.pinned;
              return !!fixture.pinned;
            }
            if (cmd === "compact_control") {
              if (args.preview && fixture.failBrightnessPreview) {
                fixture.failBrightnessPreview = false;
                throw Error("Transient preview failure");
              }
              if (typeof args.brightness === "number")
                state.settings.brightness = args.brightness;
              if (args.mode) {
                state.settings.mode = args.mode;
                state.requested = state.powered = true;
                state.status.connected = true;
                state.capturing = ["screen", "music", "blend"].includes(
                  args.mode,
                );
              }
              if (args.toggle)
                state.requested = state.powered = !state.requested;
              return structuredClone(state);
            }
            if (cmd === "apply_profile") {
              const p = profileLibrary.profiles.find((p) => p.id === args.id);
              state.settings = {
                ...state.settings,
                ...structuredClone(p.settings),
                screenId: p.source.screenId,
              };
              state.requested = true;
              state.powered = true;
              return structuredClone(state);
            }
            if (cmd === "diagnostic_report")
              return {
                text: "DynamicLight test report: no private data",
                enabled: !!args.enabled,
              };
            if (cmd === "export_diagnostics")
              return "Downloads/DynamicLight-diagnostics-test.txt";
            if (cmd === "platform_name") return "windows";
            if (cmd === "app_version") return appVersion;
            if (cmd === "launch_at_login") {
              if (fixture.failLogin) {
                fixture.failLogin = false;
                throw new Error("Test login registration failed");
              }
              if (args.enabled !== null) fixture.login = args.enabled;
              return !!fixture.login;
            }
            if (cmd === "displays")
              return [{ id: "test", name: "Test display" }];
            if (cmd !== "control") return;
            const a = args.action;
            calls.push(structuredClone(a));
            if (a.type === "routine") {
              state.routines ||= {
                sceneRevision: 0,
                timerPhase: "",
                timerPaused: false,
                remainingSeconds: 0,
                cycle: 0,
              };
              if (a.command === "start")
                Object.assign(state.routines, {
                  timerPhase: "Focus",
                  remainingSeconds: routineConfig.focusMinutes * 60,
                  cycle: 1,
                  timerPaused: false,
                });
              if (a.command === "pause")
                state.routines.timerPaused = !state.routines.timerPaused;
              if (a.command === "skip") state.routines.timerPhase = "Break";
              if (a.command === "cancel") state.routines.timerPhase = "";
            }
            if (a.type === "recordStart")
              state.sequence = {
                ...state.sequence,
                recording: true,
                elapsedMs: 0,
                recordedFrames: 0,
              };
            if (a.type === "recordStop")
              state.sequence = {
                recording: false,
                clip: {
                  id: "recorded",
                  count: state.settings.count,
                  sides: state.settings.sides,
                  durationMs: 1000,
                  frames: 30,
                },
              };
            if (
              a.type === "patchSettings" &&
              a.patch.mode === "replay" &&
              !a.patch.replay.remap &&
              state.sequence.clip.count !== state.settings.count
            )
              throw Error("Explicit remapping required");
            await new Promise((resolve) => setTimeout(resolve, 35));
            if (a.type === "calibrationStep" && !state.calibrating)
              throw new Error("Start calibration first");
            if (a.type === "calibrationStart") {
              calibrationOriginal = structuredClone(state);
              state.calibrating = true;
            }
            if (a.type === "calibrationEnd") {
              const layout = state.settings;
              Object.assign(state, calibrationOriginal);
              if (a.apply)
                Object.assign(state.settings, {
                  count: layout.count,
                  sides: layout.sides,
                  reverse: layout.reverse,
                  offset: layout.offset,
                });
              state.calibrating = false;
            }
            if (a.type === "selectMode") {
              state.settings.mode = a.mode;
              state.capturing = false;
            }
            if (a.type === "startMode") {
              state.settings.mode = a.mode;
              state.capturing = false;
            }
            if (a.type === "patchSettings")
              a.settings = { ...state.settings, ...structuredClone(a.patch) };
            if (a.settings) {
              if (
                state.settings.mode !== a.settings.mode ||
                state.settings.audioSource !== a.settings.audioSource ||
                state.settings.screenId !== a.settings.screenId
              )
                state.capturing = false;
              state.settings = structuredClone(a.settings);
            }
            if (a.type === "start" || a.type === "startMode") {
              state.requested = true;
              state.powered = true;
              state.error = "";
              state.recovering = false;
              state.captureRequiresStart = false;
              if (fixture.failConnect) {
                fixture.failConnect = false;
                state.error = "Test USB unavailable";
                state.recovering = true;
                return structuredClone(state);
              }
              state.status = {
                connected: true,
                message: "Test fixture controller",
              };
              if (fixture.failCapture) {
                fixture.failCapture = false;
                state.error = "Test capture denied";
                state.captureRequiresStart = true;
                return structuredClone(state);
              }
              state.capturing = ["screen", "music", "blend"].includes(
                state.settings.mode,
              );
            }
            if (a.type === "connect" && fixture.failConnect) {
              fixture.failConnect = false;
              throw new Error("Test USB unavailable");
            }
            if (a.type === "startCapture" && fixture.failCapture) {
              fixture.failCapture = false;
              throw new Error("Test capture denied");
            }
            if (a.type === "connect")
              state.status = {
                connected: true,
                message: "Test fixture controller",
              };
            if (a.type === "disconnect") {
              state.requested = false;
              state.status.connected = false;
              state.capturing = false;
            }
            if (a.type === "power") {
              state.powered = a.on;
              state.requested = a.on;
              state.error = "";
              state.recovering = false;
              state.captureRequiresStart = false;
              if (!a.on) state.capturing = false;
            }
            if (a.type === "startCapture") state.capturing = true;
            if (a.type === "stopCapture") state.capturing = false;
            colors();
            return structuredClone(state);
          },
        },
      };
    }, require("../package.json").version);
    await page.goto(`http://127.0.0.1:${server.address().port}`);
    await page.waitForFunction(
      () => typeof settings !== "undefined" && settings,
    );
    assert.match(await page.locator("#frame-rate").innerText(), /Preview only/);
    assert.equal(
      await page.evaluate(() =>
        fixture.calls.some((a) =>
          ["connect", "startCapture", "start"].includes(a.type),
        ),
      ),
      false,
      "Startup never captures implicitly",
    );
    assert.equal(
      await page.locator("[data-app-version]").innerText(),
      "v" + require("../package.json").version,
    );
    for (const view of ["scenes", "device", "settings", "control"]) {
      await page.locator('.app-sidebar [data-view="' + view + '"]').click();
      assert(await page.locator("#view-" + view).isVisible());
    }
    assert.equal(
      await page.evaluate(() =>
        fixture.calls.some((a) =>
          ["start", "startMode", "connect", "startCapture"].includes(a.type),
        ),
      ),
      false,
      "Navigation never activates lighting or capture",
    );
    // Drag bursts must coalesce, apply immediately, and persist once at idle.
    await page.evaluate(() => {
      fixture.calls.length = 0;
      for (let v = 1; v <= 100; v++) {
        const slider = document.getElementById("brightness");
        slider.value = v;
        slider.dispatchEvent(new Event("input"));
      }
    });
    await page.waitForFunction(
      () => document.getElementById("save-state").textContent === "Saved",
    );
    const edits = await page.evaluate(() => fixture.calls);
    assert(
      edits.filter((a) => a.type === "patchSettings" && !a.persist).length <= 3,
      "Drag updates coalesced",
    );
    assert.equal(
      edits.filter((a) => a.type === "patchSettings" && a.persist).length,
      1,
      "One persistence write",
    );
    assert.deepEqual(edits.at(-1).patch, {});
    assert.equal(
      await page.evaluate(() => fixture.state.settings.brightness),
      100,
    );
    // A different window changes an unrelated field between preview and save.
    // Studio must commit the worker's current state, not its earlier full draft.
    await page.evaluate(() => {
      const slider = document.getElementById("brightness");
      slider.value = 63;
      slider.dispatchEvent(new Event("input"));
    });
    await page.waitForFunction(
      () => fixture.state.settings.brightness === 63 && !previewPending,
    );
    await page.evaluate(() => {
      fixture.state.settings.color = "#123456";
      fixture.emitEvent("external-settings", structuredClone(fixture.state));
    });
    await page.waitForFunction(
      () => document.getElementById("save-state").textContent === "Saved",
    );
    assert.equal(
      await page.evaluate(() => fixture.state.settings.brightness),
      63,
    );
    assert.equal(
      await page.evaluate(() => fixture.state.settings.color),
      "#123456",
    );
    // For the same field, a later external edit wins even if Studio's delayed
    // persistence timer has not fired yet.
    await page.evaluate(() => {
      const slider = document.getElementById("brightness");
      slider.value = 48;
      slider.dispatchEvent(new Event("input"));
    });
    await page.waitForFunction(
      () => fixture.state.settings.brightness === 48 && !previewPending,
    );
    await page.evaluate(() => {
      fixture.state.settings.brightness = 81;
      fixture.emitEvent("external-settings", structuredClone(fixture.state));
    });
    await page.waitForFunction(
      () => document.getElementById("save-state").textContent === "Saved",
    );
    assert.equal(
      await page.evaluate(() => fixture.state.settings.brightness),
      81,
    );
    assert.equal(await page.locator("#brightness").inputValue(), "81");
    await page.locator('[data-mode="rainbow"]').click();
    await page.waitForFunction(() => !busy);
    assert(
      await page.evaluate(
        () => fixture.state.status.connected && fixture.state.powered,
      ),
      "One effect-mode click connects and powers on",
    );
    for (const effect of ["rainbow", "aurora", "breath", "comet", "ember"]) {
      await page.locator(`[data-effect="${effect}"]`).click();
      await page.waitForFunction(
        (effect) => fixture.state.settings.effect === effect,
        effect,
      );
      await page.waitForFunction(() => !busy);
      assert.equal(
        await page.locator("#palette").isDisabled(),
        ["rainbow", "ember"].includes(effect),
      );
    }
    await page.locator('[data-effect="aurora"]').click();
    await page.waitForFunction(
      () => document.getElementById("save-state").textContent === "Saved",
    );
    await page.waitForFunction(() => fixture.previewCalls > 2);
    await page.emulateMedia({ reducedMotion: "reduce" });
    await page.waitForTimeout(300);
    const frozenCalls = await page.evaluate(() => fixture.previewCalls);
    await page.waitForTimeout(350);
    assert.equal(
      await page.evaluate(() => fixture.previewCalls),
      frozenCalls,
      "Reduced motion stops thumbnail animation",
    );
    await page.emulateMedia({ reducedMotion: "no-preference" });
    fs.mkdirSync(".local", { recursive: true });
    await page.screenshot({ path: ".local/ui-effects.png", fullPage: true });
    await page.setViewportSize({ width: 1120, height: 780 });
    await page.waitForTimeout(70);
    assert(
      await page
        .locator(".effect-grid")
        .evaluate(
          (el) =>
            el.getBoundingClientRect().bottom <=
            document.querySelector(".stage-column").getBoundingClientRect()
              .bottom,
        ),
      "All effect labels fit at the default window size",
    );
    // Check visual proportions, not just overflow: maximized and ultrawide
    // displays previously flattened the monitor and stretched every audio bar.
    await page.locator('[data-mode="blend"]').click();
    await page.waitForFunction(() => !busy);
    const cdp = await page.context().newCDPSession(page);
    for (const [width, height, scale] of [
      [1120, 780, 1],
      [1920, 1080, 1],
      [2560, 1440, 1],
      [3832, 2092, 1],
      [3440, 1440, 1],
      [3066, 1674, 1.25],
      [2555, 1395, 1.5],
      [1916, 1046, 2],
      [1180, 600, 1],
    ]) {
      await page.setViewportSize({ width, height });
      await cdp.send("Emulation.setDeviceMetricsOverride", {
        width,
        height,
        deviceScaleFactor: scale,
        mobile: false,
      });
      await page.waitForTimeout(70);
      const geometry = await page.evaluate(() => {
        const box = (selector) => {
          const r = document.querySelector(selector).getBoundingClientRect();
          return {
            left: r.left,
            right: r.right,
            top: r.top,
            bottom: r.bottom,
            width: r.width,
            height: r.height,
          };
        };
        const canvas = document.getElementById("preview");
        // Observe the actual path sent to Canvas, including at wide/short sizes.
        const ctx = canvas.getContext("2d"),
          points = [];
        const move = ctx.moveTo,
          line = ctx.lineTo;
        ctx.moveTo = ctx.lineTo = function (x, y) {
          points.push([x, y]);
        };
        try {
          draw(runtime.colors);
        } finally {
          ctx.moveTo = move;
          ctx.lineTo = line;
        }
        const xs = points.map((p) => p[0]),
          ys = points.map((p) => p[1]);
        const monitorWidth = Math.max(...xs) - Math.min(...xs);
        const monitorHeight = Math.max(...ys) - Math.min(...ys);
        const spectrum = document.getElementById("spectrum");
        const audio = spectrum.getContext("2d"),
          bars = [],
          round = audio.roundRect;
        audio.roundRect = function (...rect) {
          bars.push(rect);
          return round.apply(this, rect);
        };
        try {
          drawSpectrum(Array(32).fill(0.5));
        } finally {
          audio.roundRect = round;
        }
        return {
          windowWidth: innerWidth,
          windowHeight: innerHeight,
          scale: devicePixelRatio,
          workspace: box(".workspace"),
          sidebar: box(".app-sidebar"),
          inspector: box(".controls-column"),
          preview: box("#preview"),
          power: box("#power"),
          modes: box(".stage-column .modes"),
          monitorRatio: monitorWidth / monitorHeight,
          inside:
            Math.min(...xs) >= 0 &&
            Math.max(...xs) <= canvas.clientWidth &&
            Math.min(...ys) >= 0 &&
            Math.max(...ys) <= canvas.clientHeight,
          bars: bars.map(([x, y, w, h]) => ({ x, y, w, h })),
          spectrumWidth: spectrum.clientWidth,
          spectrumHeight: spectrum.clientHeight,
          crisp:
            spectrum.width ===
            Math.round(spectrum.clientWidth * Math.min(devicePixelRatio, 2)),
          overflow: document.documentElement.scrollWidth > innerWidth,
        };
      });
      const label = `${width}×${height} at ${scale}x`;
      assert(
        geometry.windowWidth === width &&
          geometry.windowHeight === height &&
          geometry.scale === scale,
        label + " uses the requested viewport",
      );
      assert(
        !geometry.overflow && geometry.inside,
        label + " stays within the window",
      );
      assert(
        Math.abs(geometry.monitorRatio - 16 / 9) < 0.001,
        label + " preserves monitor proportions",
      );
      assert(
        geometry.workspace.width <= 1601 && geometry.preview.width <= 1229,
        label + " bounds the work area",
      );
      assert(
        Math.abs(geometry.workspace.left - geometry.sidebar.right) < 2,
        label + " keeps navigation attached to the work area",
      );
      assert(
        geometry.inspector.left - geometry.modes.right < 40,
        label + " keeps the inspector beside the workspace",
      );
      assert(
        geometry.inspector.width >= 272 && geometry.inspector.width <= 320,
        label + " keeps settings readable",
      );
      assert(geometry.power.bottom < height, label + " keeps Stop accessible");
      assert(
        geometry.crisp && geometry.bars.length === 32,
        label + " draws all frequency bands at display resolution",
      );
      assert(
        geometry.bars.every(
          (b) =>
            b.w <= 16 &&
            b.x >= 0 &&
            b.x + b.w <= geometry.spectrumWidth &&
            b.y + b.h <= geometry.spectrumHeight,
        ),
        label + " never stretches the audio bars",
      );
      if (width === 3832) {
        await page.emulateMedia({ colorScheme: "dark" });
        await page.waitForTimeout(150);
        await page.screenshot({ path: ".local/ui-maximized.png" });
      }
    }
    await cdp.send("Emulation.clearDeviceMetricsOverride");
    await cdp.detach();
    await page.locator('[data-mode="rainbow"]').click();
    await page.waitForFunction(() => !busy);
    for (const width of [1440, 1180, 880, 650, 540, 390]) {
      await page.setViewportSize({ width, height: 900 });
      assert(
        await page.evaluate(
          () => document.documentElement.scrollWidth <= innerWidth,
        ),
        `No horizontal overflow at ${width}px`,
      );
      if (width <= 760) {
        assert(
          await page.locator("#preview-toggle").isVisible(),
          "Large preview has an explicit narrow-window toggle",
        );
        assert.equal(await page.locator(".preview-card").isVisible(), false);
        assert(
          await page.locator(".controls-column").isVisible(),
          "Mode controls remain available",
        );
        assert(
          await page.locator("#power").isVisible(),
          "Stop stays in the fixed header",
        );
      }
      if (width === 540)
        await page.screenshot({
          path: ".local/ui-compact.png",
          fullPage: true,
        });
    }
    await page.setViewportSize({ width: 1280, height: 1000 });
    await page.locator('[data-mode="music"]').click();
    await page.waitForFunction(() => !busy);
    assert.equal(await page.locator("#music-controls").isVisible(), true);
    assert(
      await page.evaluate(() => fixture.state.capturing),
      "Music starts with one mode click",
    );
    assert.equal(await page.locator("#display-controls").isVisible(), false);
    for (const style of ["spectrum", "bass", "vu", "wave"]) {
      await page.locator(`[data-music="${style}"]`).click();
      await page.waitForFunction(
        (style) => fixture.state.settings.musicStyle === style,
        style,
      );
      await page.waitForFunction(() => !busy);
    }
    await page.locator("#power").click();
    await page.waitForFunction(() => !busy);
    await page.locator("#power").click();
    await page.waitForFunction(() => !busy);
    assert(
      await page.evaluate(
        () =>
          fixture.state.status.connected &&
          fixture.state.powered &&
          fixture.state.capturing,
      ),
      "Capture auto connects and powers on",
    );
    await page.locator("#audio-source").selectOption("microphone");
    await page.waitForFunction(() => !busy);
    assert(
      await page.evaluate(
        () =>
          fixture.state.capturing &&
          fixture.state.settings.audioSource === "microphone",
      ),
      "Active source change restarts capture",
    );
    await page.evaluate(() => {
      Object.assign(fixture.state, {
        fps: 29,
        framesWritten: 420,
        audioSamples: 48000,
        audioLevel: 0.25,
        lastWriteMs: 24,
        writeDurationMs: 1.2,
        writeErrors: 2,
        audioBands: Array.from(
          { length: 32 },
          (_, i) => 0.2 + 0.65 * Math.abs(Math.sin(i / 4)),
        ),
      });
      fixture.emit();
    });
    await page.locator("#connection-details").click();
    assert.equal(await page.locator("#usb-fps").innerText(), "29");
    assert.equal(await page.locator("#audio-level").innerText(), "25% RMS");
    assert.equal(await page.locator("#write-errors").innerText(), "2");
    await page.screenshot({
      path: ".local/ui-music-fixture.png",
      fullPage: true,
    });
    await page.locator('[data-close="connection-dialog"]').click();
    await page.locator('[data-mode="screen"]').click();
    await page.waitForFunction(() => !busy);
    assert.equal(await page.locator("#display-controls").isVisible(), true);
    assert.equal(await page.locator("#music-controls").isVisible(), false);
    await page.locator("#refresh-screens").click();
    await page.waitForFunction(() => !busy);
    assert.equal(await page.locator("#screen").inputValue(), "");
    await page.locator("#screen").selectOption("test");
    await page.waitForFunction(() => !busy);
    assert(
      await page.evaluate(
        () =>
          fixture.state.capturing && fixture.state.settings.screenId === "test",
      ),
      "Active display change resumes capture",
    );
    await page.locator('.app-sidebar [data-view="device"]').click();
    assert.equal(await page.locator("#count").isVisible(), true);
    await page.evaluate(() => {
      fixture.state.status.connected = false;
      fixture.state.capturing = false;
      fixture.state.powered = false;
      fixture.state.requested = false;
      fixture.failConnect = true;
      fixture.emit();
    });
    await page.locator("#power").click();
    await page.waitForFunction(() => !busy);
    assert.match(
      await page.locator("#notice").innerText(),
      /Test USB unavailable/,
    );
    assert.match(await page.locator("#power").innerText(), /Stop/);
    await page.locator("#retry").click();
    await page.waitForFunction(() => !busy);
    assert(
      await page.evaluate(() => fixture.state.capturing),
      "Retry recovers in one click",
    );
    await page.locator("#power").click();
    await page.waitForFunction(() => !busy);
    await page.evaluate(() => {
      fixture.failCapture = true;
    });
    await page.locator("#power").click();
    await page.waitForFunction(() => !busy);
    assert.match(
      await page.locator("#notice").innerText(),
      /Test capture denied/,
    );
    assert.match(await page.locator("#power").innerText(), /Stop/);
    // Error text is escaped; recovery removes only the previous engine error.
    await page.evaluate(() => {
      fixture.state.error = "<img src=x onerror=alert(1)>";
      fixture.emit();
    });
    assert.equal(await page.locator("#notice img").count(), 0);
    await page.evaluate(() => {
      fixture.state.error = "";
      fixture.state.captureRequiresStart = false;
      fixture.emit();
    });
    assert.equal(await page.locator("#notice").isVisible(), false);
    // Stop must remain available during automatic recovery and paused states.
    await page.evaluate(() => {
      fixture.state.recovering = true;
      fixture.state.status.connected = false;
      fixture.state.capturing = false;
      fixture.emit();
    });
    assert.match(await page.locator("#power").innerText(), /Stop/);
    await page.locator("#power").click();
    await page.waitForFunction(() => !busy);
    assert.equal(await page.evaluate(() => fixture.state.requested), false);
    assert.equal(await page.locator("#retry").isVisible(), false);
    await page.evaluate(() => {
      fixture.state.requested = true;
      fixture.state.pauseReason = "Computer locked";
      fixture.emit();
    });
    assert.match(await page.locator("#notice").innerText(), /Computer locked/);
    assert.equal(await page.locator("#retry").isVisible(), false);
    await page.locator("#power").click();
    await page.waitForFunction(() => !busy);
    await page.evaluate(() => {
      fixture.state.pauseReason = "";
      fixture.emit();
    });
    assert.equal(await page.evaluate(() => fixture.state.requested), false);
    // Preference edits never start lighting, and login registration failures roll back.
    await page.locator('.app-sidebar [data-view="settings"]').click();
    await page.locator("#restore-on-start").check();
    await page.locator("#resume-capture").check();
    await page.waitForFunction(
      () => document.getElementById("save-state").textContent === "Saved",
    );
    assert.equal(await page.evaluate(() => fixture.state.requested), false);
    assert.equal(
      await page.evaluate(() => fixture.state.settings.resumeCapture),
      true,
    );
    await page.locator("#launch-at-login").check();
    await page.waitForFunction(() => !busy);
    assert.equal(await page.evaluate(() => fixture.login), true);
    await page.evaluate(() => {
      fixture.failLogin = true;
    });
    await page.locator("#launch-at-login").click();
    await page.waitForFunction(() => !busy);
    assert.equal(await page.locator("#launch-at-login").isChecked(), true);
    assert.match(
      await page.locator("#notice").innerText(),
      /Test login registration failed/,
    );
    for (const width of [1180, 650, 390]) {
      await page.setViewportSize({ width, height: 900 });
      assert(
        await page.evaluate(
          () => document.documentElement.scrollWidth <= innerWidth,
        ),
        `Behavior settings fit at ${width}px`,
      );
    }
    await page.locator('.app-sidebar [data-view="control"]').click();
    // New controls use one atomic Start-mode action.
    await page.locator('[data-mode="blend"]').click();
    await page.waitForFunction(() => !busy);
    assert.equal(await page.locator("#music-controls").isVisible(), true);
    assert.equal(await page.locator("#display-controls").isVisible(), true);
    await page.locator("#screen-tuning summary").click();
    await page.locator("#exclude-bars").check();
    await page.locator(".advanced-audio > summary").click();
    await page.locator("#input-gain").fill("6");
    await page.locator("#input-gain").dispatchEvent("input");
    await page.locator("#blend").fill("75");
    await page.locator("#blend").dispatchEvent("input");
    await page.waitForTimeout(800);
    assert.equal(
      await page.evaluate(() => fixture.state.settings.inputGain),
      6,
    );
    assert.equal(await page.evaluate(() => fixture.state.settings.blend), 75);
    assert.equal(
      await page.evaluate(() => fixture.state.settings.excludeBars),
      true,
    );
    await page.locator('[data-mode="music"]').click();
    await page.waitForFunction(() => !busy);
    await page.locator('[data-music="beat"]').click();
    await page.waitForFunction(() => !busy);
    assert.equal(
      await page.evaluate(() => fixture.state.settings.musicStyle),
      "beat",
    );
    await page.locator('[data-mode="blend"]').click();
    await page.waitForFunction(() => !busy);
    await page.locator(".workspace-heading [data-save-scene]").click();
    await page.locator("#profile-name").fill("Movies");
    await page.locator("#profile-save").click();
    await page.waitForFunction(() => !busy);
    await page.locator('.app-sidebar [data-view="scenes"]').click();
    assert.equal(await page.locator(".profile-row").count(), 1);
    await page
      .getByRole("button", { name: "Edit Movies", exact: true })
      .click();
    await page
      .locator("#scene-editor")
      .getByRole("button", { name: "Duplicate", exact: true })
      .click();
    await page.waitForFunction(() => !busy);
    assert.equal(await page.locator(".profile-row").count(), 2);
    await page.locator(".profile-apply").first().click();
    await page.waitForFunction(() => !busy);
    assert.equal(
      await page.evaluate(() => fixture.state.settings.mode),
      "blend",
    );
    await page.waitForFunction(() =>
      document
        .querySelector(".profile-apply")
        .classList.contains("scene-current"),
    );
    assert.equal(
      await page.evaluate(() => "screenId" in library.profiles[0].settings),
      false,
    );
    assert.equal(
      await page.evaluate(() => library.profiles[0].source.screenId),
      await page.evaluate(() => fixture.state.settings.screenId),
    );
    await page.locator('.app-sidebar [data-view="device"]').click();
    await page.locator("#calibrate").click();
    await page.waitForFunction(() => !busy);
    assert.equal(await page.locator("#calibration-dialog").isVisible(), true);
    await page.locator("#cal-count").fill("32");
    await page.locator("#cal-count").dispatchEvent("change");
    await page.waitForTimeout(100);
    await page.locator("#cal-next").click();
    await page.waitForTimeout(100);
    assert.match(await page.locator("#cal-status").innerText(), /LED 2 of 32/);
    await page.locator("#cal-preview").click();
    await page.waitForTimeout(100);
    await page.locator("#cal-apply").click();
    await page.waitForFunction(
      () => !document.querySelector("#calibration-dialog").open,
    );
    assert.equal(await page.evaluate(() => fixture.state.settings.count), 32);
    await page.locator("#calibrate").click();
    await page.waitForFunction(() => !busy);
    await page.locator("#cal-count").fill("12");
    await page.locator("#cal-count").dispatchEvent("change");
    await page.waitForTimeout(100);
    await page.keyboard.press("Escape");
    await page.waitForFunction(
      () => !document.querySelector("#calibration-dialog").open,
    );
    assert.equal(await page.evaluate(() => fixture.state.settings.count), 32);
    await page.evaluate(() => queue);
    await page.evaluate(() => {
      fixture.state.settings.brightness = 25;
      fixture.emitEvent("external-settings", structuredClone(fixture.state));
    });
    await page.waitForTimeout(200);
    await page.waitForFunction(
      () => document.querySelector("#brightness").value === "25",
    );
    for (const width of [390, 600, 1180]) {
      await page.setViewportSize({ width, height: 1000 });
      assert(
        await page.evaluate(
          () => document.documentElement.scrollWidth <= innerWidth,
        ),
      );
    }
    await page.setViewportSize({ width: 1180, height: 1000 });
    await page.screenshot({ path: ".local/ui-lifecycle.png", fullPage: true });
    if (!(await page.locator("#diagnostic-preview").isVisible()))
      await page.locator("#connection-details").click();
    await page.locator("#diagnostic-preview").click();
    await page.waitForFunction(
      () => document.querySelector("#diagnostic-dialog").open,
    );
    assert.match(
      await page.locator("#diagnostic-text").inputValue(),
      /no private data/,
    );
    await page.locator("#diagnostic-enabled").check();
    await page.waitForTimeout(100);
    await page.locator("#diagnostic-clear").click();
    await page.waitForFunction(
      () =>
        document.querySelector("#diagnostic-status").textContent ===
        "History cleared.",
    );
    await page.locator("#diagnostic-export").click();
    await page.waitForFunction(() =>
      document
        .querySelector("#diagnostic-status")
        .textContent.startsWith("Saved to "),
    );
    await page.setViewportSize({ width: 390, height: 800 });
    await page.screenshot({ path: ".local/ui-diagnostics.png" });
    assert(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    );
    await page.keyboard.press("Escape");
    assert.equal(await page.locator("#diagnostic-dialog").isVisible(), false);
    await page.locator('[data-close="connection-dialog"]').click();
    await page.locator('.app-sidebar [data-view="scenes"]').click();
    assert.equal(await page.locator(".starter-card").count(), 4);
    await page.locator("#scene-search").fill("ocean");
    assert.equal(await page.locator(".scene-card").count(), 1);
    await page.locator(".starter-card .profile-apply").click();
    await page.waitForFunction(() => !busy);
    assert.equal(
      await page.evaluate(() => fixture.state.settings.palette),
      "ocean",
    );
    await page
      .locator(".starter-card")
      .getByRole("button", { name: "Save a copy" })
      .click();
    await page.waitForFunction(() => !busy);
    await page.locator("#scene-filter").selectOption("favorites");
    assert.equal(await page.locator(".profile-row").count(), 1);
    assert.equal(await page.locator(".starter-card").count(), 0);
    await page.locator("#scene-search").fill("");
    await page.locator("#scene-filter").selectOption("all");
    await page.setViewportSize({ width: 1180, height: 1000 });
    await page.locator("#scene-search").scrollIntoViewIfNeeded();
    await page.waitForFunction(() => fixture.sceneCalls > 0);
    await page.waitForTimeout(300);
    await page.screenshot({ path: ".local/ui-scenes.png" });
    await page.emulateMedia({ reducedMotion: "reduce" });
    await page.waitForTimeout(300);
    const sceneCalls = await page.evaluate(() => fixture.sceneCalls);
    await page.waitForTimeout(400);
    assert.equal(await page.evaluate(() => fixture.sceneCalls), sceneCalls);
    await page.evaluate(() => {
      fixture.sceneDelay = 150;
      lastScenePreviewKey = "";
      refreshScenePreviews();
      renderProfiles();
    });
    await page.waitForFunction(() =>
      [...document.querySelectorAll(".scene-art")].some((c) =>
        c
          .getContext("2d")
          .getImageData(0, 0, c.width, c.height)
          .data.some((v, i) => i % 4 === 3 && v > 0),
      ),
    );
    await page.evaluate(() => {
      fixture.sceneDelay = 0;
    });
    await page.locator('.app-sidebar [data-view="control"]').click();
    await page.locator('[data-mode="solid"]').click();
    await page.waitForFunction(() => !busy);
    const oldColor = await page.evaluate(() => settings.color);
    await page.locator("#open-color-editor").click();
    await page.locator("#color-hex").fill("#ffffff");
    await page.locator("#color-hex").dispatchEvent("input");
    const wheel = await page.locator("#color-wheel").boundingBox();
    await page.mouse.click(
      wheel.x + wheel.width * 0.9,
      wheel.y + wheel.height * 0.5,
    );
    assert.notEqual(
      await page.locator("#color-hex").inputValue(),
      "#ffffff",
      "The wheel can select a color starting from white",
    );
    await page.locator("#color-hex").fill("#123456");
    await page.locator("#color-hex").dispatchEvent("input");
    assert.equal(
      await page.evaluate(() => fixture.state.settings.color),
      oldColor,
      "Draft color never changes hardware",
    );
    await page.locator("#reset-color").click();
    assert.equal(await page.locator("#color-hex").inputValue(), oldColor);
    await page.locator("#color-temperature").fill("2700");
    await page.locator("#color-temperature").dispatchEvent("input");
    assert.match(
      await page.locator("#color-temperature-value").innerText(),
      /2700 K/,
    );
    await page.locator("#close-color-editor").click();
    assert.equal(await page.evaluate(() => settings.color), oldColor);
    await page.locator("#open-color-editor").click();
    await page.locator("#color-hex").fill("#123456");
    await page.locator("#color-hex").dispatchEvent("input");
    await page.locator("#apply-color").click();
    await page.waitForFunction(
      () => document.querySelector("#save-state").textContent === "Saved",
    );
    assert.equal(
      await page.evaluate(() => fixture.state.settings.color),
      "#123456",
    );
    assert(await page.locator("#recent-colors button").count());
    await page.locator('[data-mode="rainbow"]').click();
    await page.waitForFunction(() => !busy);
    await page.locator('[data-effect="aurora"]').click();
    await page.waitForFunction(() => !busy);
    await page.locator("#open-palette-editor").click();
    await page.locator("#color-hex").fill("#abcdef");
    await page.locator("#color-hex").dispatchEvent("input");
    await page.locator("#palette-add").click();
    await page.locator("#palette-left").click();
    await page.locator("#palette-name").fill("Evening");
    await page.locator("#palette-save").click();
    await page.waitForFunction(() => library.palettes?.length === 1);
    assert.equal(
      await page.evaluate(() => library.palettes[0].colors[0]),
      "#abcdef",
    );
    await page.locator("#palette-apply").click();
    await page.waitForFunction(
      () => fixture.state.settings.palette === "custom",
    );
    assert.equal(
      await page.evaluate(() => fixture.state.settings.customPalette[0]),
      "#abcdef",
    );
    await page.locator('.app-sidebar [data-view="scenes"]').click();
    await page.locator("#share-scenes").click();
    await page.locator("#export-save").click();
    await page.waitForFunction(() => !!fixture.lastExport);
    assert.equal(
      (await page.evaluate(() => fixture.lastExport.paletteIds)).length,
      1,
    );
    assert.match(await page.locator("#export-status").innerText(), /Downloads/);
    await page.locator("#export-cancel").click();
    const bundle = await page.evaluate(() => ({
      format: "dynamiclight",
      version: 1,
      profiles: [
        {
          id: library.profiles[0].id,
          name: library.profiles[0].name,
          settings: library.profiles[0].settings,
        },
      ],
      palettes: [],
    }));
    const upload = () =>
      page.locator("#scene-file").setInputFiles({
        name: "scenes.json",
        mimeType: "application/json",
        buffer: Buffer.from(JSON.stringify(bundle)),
      });
    await upload();
    await page.waitForFunction(
      () => document.querySelector("#import-dialog").open,
    );
    assert.match(await page.locator("#import-items").innerText(), /matches/);
    await page.evaluate(() =>
      window.__TAURI__.core.invoke("profile_library", {
        library: { ...library, recent_colors: ["#000000"] },
      }),
    );
    await page.locator("#import-confirm").click();
    await page.waitForFunction(() =>
      document
        .querySelector("#import-status")
        .textContent.includes("library changed"),
    );
    await page.locator("#import-cancel").click();
    await upload();
    await page.waitForFunction(
      () => document.querySelector("#import-dialog").open,
    );
    const profileCount = await page.evaluate(() => library.profiles.length);
    await page.locator("#import-confirm").click();
    await page.waitForFunction(
      () => !document.querySelector("#import-dialog").open,
    );
    assert.equal(
      await page.evaluate(() => library.profiles.length),
      profileCount + 1,
    );
    await page.locator('.app-sidebar [data-view="control"]').click();
    await page.locator("#open-palette-editor").click();
    await page.screenshot({ path: ".local/ui-color-tools.png" });
    for (const width of [390, 540, 1180]) {
      await page.setViewportSize({ width, height: 1000 });
      const geometry = await page.evaluate(() => {
        const d = document.querySelector("#color-editor");
        return {
          client: d.clientWidth,
          scroll: d.scrollWidth,
          overflow: [...d.querySelectorAll("*")]
            .filter(
              (e) =>
                e.getBoundingClientRect().right >
                d.getBoundingClientRect().right,
            )
            .map((e) => e.id || e.className || e.tagName),
        };
      });
      assert(
        geometry.scroll <= geometry.client,
        "Responsive color editor at " + width + ": " + JSON.stringify(geometry),
      );
    }
    await page.locator("#close-color-editor").click();
    await page.setViewportSize({ width: 1120, height: 780 });
    await page.locator('.app-sidebar [data-view="settings"]').click();
    await page.locator("#update-check").click();
    assert.equal(await page.locator("#automation-enabled").isChecked(), false);
    assert.equal(await page.locator("#automation-capture").isEnabled(), false);
    await page.locator("#automation-enabled").check();
    await page.waitForFunction(() => fixture.automation?.enabled);
    assert.equal(await page.locator("#automation-capture").isChecked(), false);
    await page.locator("#automation-capture").check();
    await page.waitForFunction(() => fixture.automation?.allowCapture);
    await page.locator("#automation-rotate").click();
    await page.waitForFunction(() => fixture.automationChanges === 3);
    await page.evaluate(() => {
      fixture.failAutomation = true;
    });
    await page.locator("#automation-enabled").click();
    await page.waitForFunction(
      () => document.querySelector("#automation-enabled").checked,
    );
    assert.match(
      await page.locator("#automation-status").textContent(),
      /could not be saved/,
    );
    await page.evaluate(() => {
      fixture.failAutomation = false;
    });
    await page.locator("#automation-enabled").uncheck();
    await page.waitForFunction(() => !fixture.automation.enabled);
    assert.equal(await page.locator("#automation-rotate").isEnabled(), false);
    await page.waitForFunction(() =>
      document
        .querySelector("#update-status")
        .textContent.includes("up to date"),
    );
    await page.locator("#update-automatic").uncheck();
    await page.waitForFunction(() => !fixture.update.automatic);
    await page.evaluate(() => {
      fixture.failUpdatePreference = true;
    });
    await page.locator("#update-automatic").click();
    await page.waitForFunction(
      () => !document.querySelector("#update-automatic").checked,
    );
    assert.match(
      await page.locator("#update-status").innerText(),
      /could not be saved/,
    );
    await page.evaluate(() => {
      fixture.update.version = "99.0.0";
      fixture.update.notes = "<script>window.injected = true</script>";
    });
    await page.locator("#update-check").click();
    await page.locator("#update-install").click();
    await page.locator("#update-cancel").click();
    assert.equal(await page.evaluate(() => fixture.updateInstalls), 0);
    await page.locator("#update-install").click();
    await page.locator("#update-confirm").click();
    await page.waitForFunction(() => fixture.updateInstalls === 1);
    assert.match(
      await page.locator("#update-status").innerText(),
      /signature verification failed/,
    );
    assert.equal(await page.evaluate(() => window.injected), undefined);
    await page.evaluate(() => {
      fixture.update.phase = "downloading";
      fixture.update.progress = 42;
      fixture.update.message = "";
      fixture.emitEvent("update-status", structuredClone(fixture.update));
    });
    assert.equal(
      await page.locator("#update-progress").getAttribute("value"),
      "42",
    );
    assert.equal(await page.locator("#update-install").isDisabled(), true);
    await page.evaluate(() => {
      fixture.update.phase = "available";
      fixture.update.canInstall = false;
      fixture.emitEvent("update-status", structuredClone(fixture.update));
    });
    assert.equal(await page.locator("#update-install").isVisible(), false);
    assert.equal(await page.locator("#update-manual").isVisible(), true);
    for (const width of [540, 780, 1120]) {
      await page.setViewportSize({ width, height: 780 });
      assert(
        await page.evaluate(
          () => document.documentElement.scrollWidth <= innerWidth,
        ),
        "Update settings fit at " + width,
      );
    }
    await page.screenshot({ path: ".local/ui-updates.png" });

    await page.locator('[data-view="scenes"]').first().click();
    await page.locator("#sequence-open").click();
    await page.locator("#sequence-record").click();
    await page.waitForFunction(() => fixture.state.sequence?.recording);
    await page.locator("#sequence-stop-record").click();
    await page.waitForFunction(
      () => fixture.state.sequence?.clip?.id === "recorded",
    );
    await page.waitForFunction(
      () => !document.querySelector("#sequence-file").disabled,
    );
    await page.locator("#sequence-file").setInputFiles({
      name: "fixture-rgb.json",
      mimeType: "application/json",
      buffer: Buffer.from(
        JSON.stringify({
          format: "dynamiclight-rgb",
          version: 1,
          id: "imported",
          count: 2,
          sides: 3,
          durationMs: 1000,
          frames: [
            {
              ms: 0,
              colors: [
                [255, 0, 0],
                [0, 0, 255],
              ],
            },
          ],
        }),
      ),
    });
    await page.waitForFunction(
      () => fixture.state.sequence?.clip?.id === "imported",
    );
    await page.locator("#sequence-play").click();
    await page.waitForFunction(() =>
      /remapping/.test(document.querySelector("#sequence-status").textContent),
    );
    await page.locator("#sequence-remap").check();
    await page.locator("#sequence-speed").fill("200");
    await page.locator("#sequence-start").fill("0.2");
    await page.locator("#sequence-play").click();
    await page.waitForFunction(() =>
      /Offline replay started/.test(
        document.querySelector("#sequence-status").textContent,
      ),
    );
    assert.equal(
      await page.evaluate(() => fixture.state.settings.replay.startMs),
      200,
    );
    assert.equal(
      await page.evaluate(() => fixture.state.settings.replay.speed),
      200,
    );
    assert.equal(
      await page.evaluate(() => fixture.state.capturing),
      false,
      "Offline playback has no capture",
    );
    await page.locator("#sequence-stop").click();
    await page.waitForFunction(() => !fixture.state.requested);
    await page.locator("#sequence-close").click();
    await page.locator("#routine-open").click();
    await page.locator("#routine-focus").fill("10");
    await page.locator("#routine-break").fill("2");
    await page.locator("#routine-cycles").fill("3");
    await page.locator("#routine-manualCue-enabled").check();
    await page.locator("#routine-add-schedule").click();
    await page.locator("#routine-refresh-apps").click();
    await page.locator("#routine-app").selectOption("Fixture editor");
    await page.locator("#routine-add-rule").click();
    await page.locator("#routine-apps-enabled").check();
    await page.locator("#routine-paused").uncheck();
    await page.locator("#routine-start").click();
    await page.waitForFunction(
      () => fixture.state.routines?.timerPhase === "Focus",
    );
    await page.waitForFunction(() =>
      /10:00/.test(document.querySelector("#routine-timer-status").textContent),
    );
    const savedRoutines = await page.evaluate(() =>
      window.__TAURI__.core.invoke("routines_config", { config: null }),
    );
    assert.equal(savedRoutines.schedules.length, 1);
    assert.equal(savedRoutines.appRules[0].app, "Fixture editor");
    assert.equal(savedRoutines.cycles, 3);
    await page.locator("#routine-pause-timer").click();
    await page.waitForFunction(() => fixture.state.routines.timerPaused);
    await page.locator("#routine-skip").click();
    await page.waitForFunction(
      () => fixture.state.routines.timerPhase === "Break",
    );
    await page.locator("#routine-cancel").click();
    await page.waitForFunction(() => !fixture.state.routines.timerPhase);
    await page.locator("#routine-close").click();
    // Capture source changes remain a deliberate, local selection.
    await page.locator('[data-view="control"]').click();
    await page.locator('[data-mode="screen"]').click();
    await page.waitForFunction(() => !busy);
    await page.locator("#source-open").click();
    await page.locator("#source-kind").selectOption("region");
    await page.locator("#source-left").fill("75");
    await page.locator("#source-width").fill("50");
    await page.locator("#source-apply").click();
    assert.match(await page.locator("#source-status").innerText(), /inside/);
    await page.locator("#source-left").fill("25");
    await page.locator("#source-width").fill("50");
    await page.locator("#source-apply").click();
    await page.waitForFunction(
      () => !document.querySelector("#source-dialog").open,
    );
    assert.deepEqual(
      await page.evaluate(() => fixture.state.settings.captureSource),
      { kind: "region", left: 250, top: 0, width: 500, height: 1000 },
    );
    await page.locator("#source-open").click();
    await page.locator("#source-kind").selectOption("window");
    await page.locator("#source-refresh").click();
    await page.locator("#source-windows").selectOption("0");
    await page.locator("#source-apply").click();
    await page.waitForFunction(
      () => !document.querySelector("#source-dialog").open,
    );
    assert.equal(
      await page.evaluate(() => fixture.state.settings.captureSource.id),
      123,
    );
    await page.locator('[data-view="settings"]').click();
    // Appearance preferences cover every supported palette and both color schemes.
    await page.setViewportSize({ width: 1120, height: 780 });
    for (const theme of ["light", "dark"])
      for (const accent of ["violet", "blue", "green", "rose"]) {
        await page.locator("#appearance-theme").selectOption(theme);
        await page.locator("#appearance-accent").selectOption(accent);
        await page.waitForFunction(
          ({ theme, accent }) =>
            document.documentElement.dataset.theme === theme &&
            document.documentElement.dataset.accent === accent,
          { theme, accent },
        );
        const contrast = await page.evaluate(() => {
          const css = getComputedStyle(document.documentElement);
          const luminance = (name) => {
            const hex = css.getPropertyValue(name).trim().slice(1);
            const c = [0, 2, 4]
              .map((i) => parseInt(hex.slice(i, i + 2), 16) / 255)
              .map((c) =>
                c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4,
              );
            return c[0] * 0.2126 + c[1] * 0.7152 + c[2] * 0.0722;
          };
          return [
            ["--text", "--panel"],
            ["--muted", "--panel"],
            ["--accent", "--accent-soft"],
            ["--accent-text", "--accent"],
          ].map(([a, b]) => {
            const x = luminance(a),
              y = luminance(b);
            return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05);
          });
        });
        assert(
          contrast.every((c) => c >= 4.5),
          theme + " " + accent + " text contrast: " + contrast,
        );
      }
    await page.locator("#appearance-density").selectOption("compact");
    await page.locator("#appearance-theme").selectOption("system");
    await page.emulateMedia({ colorScheme: "light" });
    await page.waitForFunction(
      () => document.documentElement.dataset.theme === "light",
    );
    await page.emulateMedia({ colorScheme: "dark" });
    await page.waitForFunction(
      () => document.documentElement.dataset.theme === "dark",
    );
    await page.locator('[data-view="control"]').click();
    await page.locator('[data-mode="rainbow"]').click();
    await page.waitForFunction(() => !busy);
    await page.locator("#open-studio-effect").click();
    const beforeStudio = await page.evaluate(() =>
      JSON.stringify(fixture.state.settings),
    );
    await page.locator("#studio-duration").fill("12");
    await page.locator("#studio-duration").dispatchEvent("change");
    await page.locator("#studio-add-layer").click();
    assert.equal(await page.locator("#studio-layers fieldset").count(), 2);
    await page.locator("#studio-undo").click();
    assert.equal(await page.locator("#studio-layers fieldset").count(), 1);
    await page.locator("#studio-cancel").click();
    assert.equal(
      await page.evaluate(() => JSON.stringify(fixture.state.settings)),
      beforeStudio,
      "Cancel preserves native scene",
    );
    await page.locator("#open-studio-effect").click();
    await page.locator("#studio-duration").fill("12");
    await page.locator("#studio-duration").dispatchEvent("change");
    await page.locator("#studio-preset-name").fill("Layered ocean");
    await page.locator("#studio-save-preset").click();
    await page.waitForFunction(
      () => fixture.studioLibrary?.effects.length === 1,
    );
    await page.locator("#studio-apply").click();
    await page.waitForFunction(
      () => !document.querySelector("#studio-dialog").open,
    );
    assert.equal(
      await page.evaluate(
        () => fixture.state.settings.studio.effect.durationMs,
      ),
      12000,
    );
    await page.locator('[data-view="device"]').click();
    await page.locator("#open-studio-zones").click();
    await page.locator("#studio-edge-top").click();
    await page.locator("#studio-add-zone").click();
    assert.equal(await page.locator("#studio-zones fieldset").count(), 1);
    await page.locator("#studio-leds button").first().focus();
    await page.keyboard.press("Shift+ArrowRight");
    assert.equal(
      await page.locator("#studio-leds [aria-pressed=true]").count(),
      2,
      "Keyboard range selection",
    );
    await page.setViewportSize({ width: 540, height: 780 });
    assert(
      await page
        .locator("#studio-dialog")
        .evaluate((e) => e.scrollWidth <= e.clientWidth),
      "Studio editor fits narrow windows",
    );
    await page.screenshot({ path: ".local/ui-studio-zones.png" });
    await page.locator("#studio-apply").click();
    await page.waitForFunction(
      () => !document.querySelector("#studio-dialog").open,
    );
    assert.equal(
      await page.evaluate(() => fixture.state.settings.studio.zones.length),
      1,
    );
    await page.locator('[data-view="control"]').click();
    await page.locator('[data-mode="solid"]').click();
    await page.waitForFunction(() => !busy);
    await page.locator("#open-studio-bias").click();
    await page.locator("#studio-red").fill("93");
    await page.locator("#studio-red").dispatchEvent("change");
    await page.locator("#studio-preset-name").fill("Metered desk");
    await page.locator("#studio-save-preset").click();
    await page.waitForFunction(() => fixture.studioLibrary.bias.length === 1);
    await page.locator("#studio-apply").click();
    await page.waitForFunction(
      () => !document.querySelector("#studio-dialog").open,
    );
    assert.equal(
      await page.evaluate(() => fixture.state.settings.studio.bias.gains[0]),
      93,
    );
    assert.equal(
      await page.evaluate(() => fixture.state.settings.studio.zones.length),
      0,
      "Bias removes decorative zones",
    );
    const colors = await page.evaluate(() =>
      ImagePalette.extract(
        new Uint8ClampedArray([
          255, 0, 0, 255, 0, 0, 255, 255, 255, 0, 0, 255, 0, 255, 0, 0,
        ]),
      ),
    );
    assert.deepEqual(
      colors,
      ["#ff0000", "#0000ff"],
      "Palette extraction ignores transparent pixels",
    );
    await page.setViewportSize({ width: 1120, height: 780 });
    await page.locator('[data-view="device"]').click();
    await page.locator("#controller-open").click();
    await page.locator("#controller-count").fill("68");
    await page.locator("#controller-offset").fill("7");
    await page.locator("#controller-reverse").check();
    await page.locator("#controller-canvas").focus();
    await page.keyboard.press("ArrowRight");
    assert.equal(await page.locator("#controller-x").inputValue(), "50");
    await page.locator("#controller-apply").click();
    await page.waitForFunction(() =>
      /Mappings applied/.test(
        document.querySelector("#controller-message").textContent,
      ),
    );
    assert.equal(await page.evaluate(() => fixture.state.settings.count), 68);
    const individualLevel = await page.evaluate(
      () => fixture.state.settings.brightness,
    );
    await page.locator("#controller-gain").fill("50");
    await page.locator("#controller-gain-apply").click();
    await page.waitForFunction(() => fixture.state.groupBrightness === 50);
    assert.equal(
      await page.evaluate(() => fixture.state.settings.brightness),
      individualLevel,
    );
    await page.locator("#ambient-enabled").check();
    await page.locator("#ambient-fallback").fill("22");
    await page.locator("#ambient-save").click();
    await page.waitForFunction(
      () => fixture.state.ambient.config.fallback === 22,
    );
    assert.equal(
      await page.evaluate(() => fixture.state.ambient.config.enabled),
      true,
    );
    await page.screenshot({ path: ".local/ui-controller-layout.png" });
    await page.locator("#controller-close").click();

    await page.locator("#extension-open").click();
    await page.locator("#extension-damage-enabled").check();
    await page.locator("#extension-damage-start").fill("250");
    await page.locator("#extension-game-enable").click();
    await page.waitForFunction(() => fixture.game?.enabled);
    assert.equal(
      await page.evaluate(() => fixture.game.config.rules[0].start),
      250,
    );
    await page.locator("#extension-game-disable").click();
    await page.waitForFunction(() => !fixture.game.enabled);
    await page.locator("#extension-sample").click();
    await page.locator("#extension-install").click();
    await page.waitForFunction(() => fixture.plugins?.length === 1);
    assert.equal(await page.evaluate(() => fixture.plugins[0].enabled), false);
    await page
      .locator("#extension-plugins")
      .getByRole("button", { name: "Enable", exact: true })
      .click();
    await page.waitForFunction(() => fixture.plugins[0].enabled);
    await page
      .locator("#extension-plugins")
      .getByRole("button", { name: "Disable", exact: true })
      .click();
    await page.waitForFunction(() => !fixture.plugins[0].enabled);
    await page
      .locator("#extension-plugins")
      .getByRole("button", { name: "Remove", exact: true })
      .click();
    await page.waitForFunction(() => fixture.plugins.length === 0);
    await page.screenshot({ path: ".local/ui-extensions.png" });
    await page.locator("#extension-close").click();
    await page.locator("#remote-open").click();
    await page.locator("#remote-address").selectOption("127.0.0.1");
    await page.locator("#remote-enable").click();
    await page.waitForFunction(() => fixture.remote.enabled);
    assert.match(await page.locator("#remote-code").innerText(), /123456/);
    await page.locator("#remote-revokeAll").click();
    await page.waitForFunction(() => !fixture.remote.pairingCode);
    await page.locator("#remote-pair").click();
    await page.waitForFunction(() => fixture.remote.pairingCode === "654321");
    await page.locator("#remote-disable").click();
    await page.waitForFunction(() => !fixture.remote.enabled);
    await page.locator("#remote-close").click();
    await page.goto(`http://127.0.0.1:${server.address().port}/compact.html`);
    await page.waitForFunction(
      () => document.querySelector("#quick-power").disabled === false,
    );
    await page.setViewportSize({ width: 360, height: 460 });
    await page.locator("#quick-brightness").fill("32");
    await page.locator("#quick-brightness").dispatchEvent("input");
    await page.waitForFunction(() => fixture.state.settings.brightness === 32);
    await page.waitForFunction(() => !brightnessPreview);
    await page.evaluate(() => {
      fixture.state.settings.brightness = 74;
    });
    await page.waitForFunction(() => !editingBrightness);
    assert.equal(
      await page.evaluate(() => fixture.state.settings.brightness),
      74,
      "Quick-controls idle save preserves later edits",
    );
    await page.evaluate(() => {
      fixture.failBrightnessPreview = true;
    });
    await page.locator("#quick-brightness").fill("46");
    await page.locator("#quick-brightness").dispatchEvent("input");
    await page.waitForFunction(() => !editingBrightness);
    assert.equal(
      await page.evaluate(() => fixture.state.settings.brightness),
      46,
      "A failed preview retries the selected value at commit",
    );
    for (const mode of ["solid", "screen", "music", "blend", "rainbow"]) {
      await page.locator('[data-quick-mode="' + mode + '"]').click();
      await page.waitForFunction(() => !busy);
      assert.equal(
        await page.evaluate(() => fixture.state.settings.mode),
        mode,
      );
      assert.equal(
        await page
          .locator('[data-quick-mode="' + mode + '"]')
          .getAttribute("aria-pressed"),
        "true",
      );
      assert.equal(
        await page.evaluate(() => fixture.state.requested),
        true,
        "Quick mode starts with one click",
      );
    }
    await page.locator("#quick-power").click();
    await page.waitForFunction(() => !fixture.state.requested);
    await page.locator("#quick-power").click();
    await page.waitForFunction(() => fixture.state.requested);
    await page.locator("#quick-power").click();
    await page.waitForFunction(() => !fixture.state.requested);
    await page.evaluate(() => {
      fixture.state.settings.brightness = 57;
      fixture.state.recovering = true;
      fixture.state.requested = true;
    });
    await page.waitForFunction(
      () => document.querySelector("#quick-value").textContent === "57%",
    );
    assert.equal(
      await page.locator("#quick-status").innerText(),
      "Reconnecting",
    );
    await page.locator("#quick-pin").check();
    await page.waitForFunction(() => fixture.pinned);
    await page.screenshot({ path: ".local/ui-compact.png" });
    for (const width of [300, 360, 540]) {
      await page.setViewportSize({ width, height: 460 });
      assert(
        await page.evaluate(
          () => document.documentElement.scrollWidth <= innerWidth,
        ),
      );
      const layout = await page.evaluate(() => ({
        modes: document.querySelector(".quick-modes").getBoundingClientRect(),
        brightness: document
          .querySelector(".quick-brightness")
          .getBoundingClientRect(),
        footer: document.querySelector(".quick-footer").getBoundingClientRect(),
        height: innerHeight,
      }));
      assert(
        layout.modes.y < layout.brightness.y,
        "Quick modes stay before brightness at " + width,
      );
      assert(
        layout.footer.bottom <= layout.height + 1,
        "Quick footer fits at " + width,
      );
    }
    assert.deepEqual(errors, []);
    console.log(
      "UI passed: profiles, calibration apply/cancel, blend, beat controls, tray synchronization, drag coalescing, effects, recovery, responsive widths and CSP.",
    );
  } finally {
    await browser?.close();
    await new Promise((resolve) => server.close(resolve));
  }
})().catch((e) => {
  console.error(e);
  process.exitCode = 1;
});
