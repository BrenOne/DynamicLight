// Isolated native WebView/IPC regression. Uses the automation-test identifier;
// no lighting Start, Connect, capture or API enable operations are issued.
const assert = require("node:assert/strict");
const path = require("node:path");
const { spawn } = require("node:child_process");
const { chromium } = require("playwright");
const delay = (ms) => new Promise((r) => setTimeout(r, ms));
(async () => {
  const child = spawn(
    path.resolve("src-tauri/target/debug/dynamiclight.exe"),
    [],
    {
      env: {
        ...process.env,
        WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: "--remote-debugging-port=9226",
      },
      stdio: "ignore",
    },
  );
  let browser,
    page,
    isolated = false,
    original,
    originalRoutines,
    originalAmbient,
    originalGroup;
  try {
    for (let i = 0; i < 60; i++) {
      if (child.exitCode !== null) throw Error("Test app exited");
      try {
        browser = await chromium.connectOverCDP("http://127.0.0.1:9226");
        break;
      } catch {
        await delay(250);
      }
    }
    assert(browser);
    for (let i = 0; i < 40; i++) {
      page = browser
        .contexts()[0]
        .pages()
        .find((p) => p.url().includes("tauri.localhost"));
      if (page) break;
      await delay(250);
    }
    await page.waitForFunction(
      () => typeof settings !== "undefined" && settings,
    );
    const invoke = (command, args) =>
      page.evaluate(
        ({ command, args }) => window.__TAURI__.core.invoke(command, args),
        { command, args },
      );
    const status = await invoke("automation_status");
    assert.equal(
      path.resolve(status.connectionFile),
      path.join(
        process.env.APPDATA,
        "com.dynamiclight.automation-test/automation/session.json",
      ),
    );
    isolated = true;
    const snapshot = await invoke("control", { action: { type: "snapshot" } });
    original = snapshot.settings;
    originalRoutines = await invoke("routines_config", { config: null });
    originalAmbient = (await invoke("ambient_config", { config: null })).config;
    originalGroup = snapshot.groupBrightness;
    assert(
      !snapshot.requested && !snapshot.capturing && !snapshot.status.connected,
    );
    const draft = structuredClone(original);
    draft.mode = "rainbow";
    draft.effect = "custom";
    draft.brightness = 100;
    draft.maxBrightness = 10;
    draft.studio.effect = {
      version: 1,
      durationMs: 1000,
      repeats: 1,
      layers: [
        {
          kind: "sequence",
          opacity: 100,
          stops: [
            { position: 0, color: "#ff0000" },
            { position: 1000, color: "#0000ff" },
          ],
        },
      ],
    };
    const frame = await invoke("studio_preview", {
      settings: draft,
      seconds: 0.5,
    });
    assert.equal(frame.length, draft.count);
    assert(frame.flat().every((v) => v <= 26));
    await assert.rejects(
      invoke("studio_preview", {
        settings: {
          ...draft,
          studio: {
            ...draft.studio,
            effect: { ...draft.studio.effect, durationMs: 0 },
          },
        },
        seconds: 0,
      }),
    );
    await page.locator('[data-view="control"]').click();
    // Select changes a stopped mode; deliberately avoid StartMode UI buttons.
    await invoke("control", {
      action: { type: "selectMode", mode: "rainbow" },
    });
    await page.evaluate(async () => {
      acceptSettings(await request({ type: "snapshot" }));
      updateUI();
    });
    await page.locator("#open-studio-effect").click();
    await page.locator("#studio-duration").fill("4");
    await page.locator("#studio-duration").dispatchEvent("change");
    await page.locator("#studio-apply").click();
    await page.waitForFunction(
      () => !document.querySelector("#studio-dialog").open,
    );
    const applied = await invoke("control", { action: { type: "snapshot" } });
    assert.equal(applied.settings.effect, "custom");
    assert.equal(applied.settings.studio.effect.durationMs, 4000);
    assert(
      !applied.requested && !applied.status.connected && !applied.capturing,
    );
    await page.locator('[data-view="settings"]').click();
    await page.locator("#appearance-theme").selectOption("dark");
    await page.locator("#appearance-accent").selectOption("green");
    await invoke("compact_window", { operation: "open", pinned: null });
    let compact;
    for (let i = 0; i < 40; i++) {
      compact = browser
        .contexts()[0]
        .pages()
        .find((p) => p.url().includes("compact.html"));
      if (compact) break;
      await delay(100);
    }
    assert(compact);
    await compact.waitForFunction(
      () =>
        document.documentElement.dataset.theme === "dark" &&
        document.documentElement.dataset.accent === "green",
    );
    await page.locator("#appearance-theme").selectOption("light");
    await compact.waitForFunction(
      () => document.documentElement.dataset.theme === "light",
    );
    await page.screenshot({ path: ".local/native-studio-settings.png" });
    await invoke("control", { action: { type: "selectMode", mode: "solid" } });
    await invoke("control", { action: { type: "recordStart", seconds: 1 } });
    await delay(200);
    const recorded = await invoke("control", {
      action: { type: "recordStop" },
    });
    assert(recorded.sequence.clip.frames > 0);
    assert(
      !recorded.requested && !recorded.capturing && !recorded.status.connected,
    );
    const playback = await invoke("control", {
      action: {
        type: "patchSettings",
        patch: {
          mode: "replay",
          replay: {
            clip: recorded.sequence.clip.id,
            startMs: 0,
            endMs: 0,
            speed: 150,
            looping: true,
            remap: false,
          },
        },
        persist: true,
      },
    });
    assert.equal(playback.settings.mode, "replay");
    assert(!playback.capturing && !playback.status.connected);
    await assert.rejects(
      invoke("import_sequence", {
        text: JSON.stringify({ format: "dynamiclight-rgb", version: 99 }),
      }),
    );
    const routineDraft = structuredClone(originalRoutines);
    routineDraft.paused = true;
    routineDraft.appsEnabled = false;
    routineDraft.focusMinutes = 1;
    routineDraft.breakMinutes = 1;
    routineDraft.manualCue.enabled = true;
    await invoke("routines_config", { config: routineDraft });
    await invoke("control", { action: { type: "routine", command: "start" } });
    await page.waitForFunction(
      async () =>
        (
          await window.__TAURI__.core.invoke("control", {
            action: { type: "snapshot" },
          })
        ).routines.timerPhase === "Focus",
    );
    await invoke("control", { action: { type: "routine", command: "pause" } });
    await page.waitForFunction(
      async () =>
        (
          await window.__TAURI__.core.invoke("control", {
            action: { type: "snapshot" },
          })
        ).routines.timerPaused,
    );
    await invoke("control", { action: { type: "routine", command: "skip" } });
    await page.waitForFunction(
      async () =>
        (
          await window.__TAURI__.core.invoke("control", {
            action: { type: "snapshot" },
          })
        ).routines.timerPhase === "Break",
    );
    await invoke("control", { action: { type: "routine", command: "cue" } });
    const routineState = await invoke("control", {
      action: { type: "snapshot" },
    });
    assert(
      !routineState.routines.cueActive &&
        !routineState.requested &&
        !routineState.status.connected &&
        !routineState.capturing,
    );
    await invoke("control", { action: { type: "routine", command: "cancel" } });
    const fleet = await invoke("controller_list");
    assert.equal(fleet.controllers.length, 1);
    const beforeGroup = await invoke("control", {
      action: { type: "snapshot" },
    });
    const groupView = await invoke("controller_group", {
      operation: "brightness",
      value: 25,
    });
    assert.equal(groupView.controllers[0].state.groupBrightness, 25);
    const grouped = await invoke("control", { action: { type: "snapshot" } });
    assert.equal(grouped.settings.brightness, beforeGroup.settings.brightness);
    assert.equal(grouped.groupBrightness, 25);
    const sensor = {
      ...originalAmbient,
      enabled: true,
      source: "external",
      fallback: 22,
      smoothing: 2,
    };
    await invoke("ambient_config", { config: sensor });
    await invoke("control", {
      action: {
        type: "patchSettings",
        patch: { brightness: 13 },
        persist: false,
      },
    });
    const overridden = await invoke("control", {
      action: { type: "snapshot" },
    });
    assert(overridden.ambient.manualOverride);
    await invoke("control", { action: { type: "ambientResume" } });
    const resumed = await invoke("control", { action: { type: "snapshot" } });
    assert(
      !resumed.ambient.manualOverride &&
        !resumed.requested &&
        !resumed.status.connected &&
        !resumed.capturing,
    );
    await assert.rejects(
      invoke("ambient_config", {
        config: { ...sensor, minimum: 90, maximum: 10 },
      }),
    );
    const scene = structuredClone(resumed.settings);
    for (const key of [
      "deviceId",
      "count",
      "sides",
      "reverse",
      "offset",
      "screenId",
      "captureSource",
      "offOnSleep",
      "offOnDisplaySleep",
      "offOnLock",
      "restoreOnStart",
      "resumeCapture",
      "offOnQuit",
      "maxBrightness",
      "transitionLimit",
      "minimumGlow",
      "modeSettings",
    ])
      delete scene[key];
    const handoff = JSON.stringify({
      format: "dynamiclight-handoff",
      version: 1,
      vendor: 0x1a86,
      product: 0xfe07,
      scene,
      count: resumed.settings.count,
      sides: resumed.settings.sides,
      offset: 3,
      reverse: true,
    });
    const preview = await invoke("handoff_preview", { text: handoff });
    assert.equal(preview.offset, 3);
    const imported = await invoke("handoff_import", { text: handoff });
    assert.equal(imported.settings.offset, 3);
    assert(!imported.requested && !imported.status.connected);
    console.log(
      "Native studio passed: bounded frames, invalid effect rejection, editor Apply while off, native persistence, RGB recording/offline setup, cross-window appearance; no USB/capture.",
    );
  } finally {
    if (page && isolated) {
      if (originalRoutines)
        await page
          .evaluate(
            (config) =>
              window.__TAURI__.core.invoke("routines_config", { config }),
            originalRoutines,
          )
          .catch(() => {});
      if (original)
        await page
          .evaluate(
            (settings) =>
              window.__TAURI__.core.invoke("control", {
                action: { type: "settings", settings },
              }),
            original,
          )
          .catch(() => {});
      if (originalAmbient)
        await page
          .evaluate(
            (config) =>
              window.__TAURI__.core.invoke("ambient_config", { config }),
            originalAmbient,
          )
          .catch(() => {});
      if (originalGroup !== undefined)
        await page
          .evaluate(
            (value) =>
              window.__TAURI__.core.invoke("controller_group", {
                operation: "brightness",
                value,
              }),
            originalGroup,
          )
          .catch(() => {});
      await page
        .evaluate(() => window.__TAURI__.core.invoke("quit"))
        .catch(() => {});
    }
    for (let i = 0; i < 20 && child.exitCode === null; i++) await delay(100);
    if (child.exitCode === null) child.kill();
    await browser?.close();
  }
})().catch((e) => {
  console.error(e);
  process.exitCode = 1;
});
