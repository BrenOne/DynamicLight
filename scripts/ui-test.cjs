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
      !["index.html", "style.css", "renderer.js", "icon.svg"].includes(name)
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
    await page.addInitScript(() => {
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
        offOnSleep: true,
        offOnDisplaySleep: true,
        offOnLock: true,
        restoreOnStart: false,
        resumeCapture: false,
        offOnQuit: true,
        effect: "aurora",
        palette: "aurora",
        speed: 35,
        musicStyle: "spectrum",
        noiseFloor: 0,
        release: 40,
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
      let listener;
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
        state,
        calls,
        emit: () => {
          colors();
          listener?.({ payload: structuredClone(state) });
        },
      };
      window.__TAURI__ = {
        event: {
          listen: async (_, callback) => {
            listener = callback;
          },
        },
        core: {
          invoke: async (cmd, args) => {
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
            if (cmd === "platform_name") return "windows";
            if (cmd === "launch_at_login") {
              if (fixture.failLogin) { fixture.failLogin = false; throw new Error("Test login registration failed"); }
              if (args.enabled !== null) fixture.login = args.enabled;
              return !!fixture.login;
            }
            if (cmd === "displays")
              return [{ id: "test", name: "Test display" }];
            if (cmd !== "control") return;
            const a = args.action;
            calls.push(structuredClone(a));
            await new Promise((resolve) => setTimeout(resolve, 35));
            if (a.settings) {
              if (
                state.settings.mode !== a.settings.mode ||
                state.settings.audioSource !== a.settings.audioSource ||
                state.settings.screenId !== a.settings.screenId
              )
                state.capturing = false;
              state.settings = structuredClone(a.settings);
            }
            if (a.type === "start") {
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
              state.status = { connected: true, message: "Test fixture controller" };
              if (fixture.failCapture) {
                fixture.failCapture = false;
                state.error = "Test capture denied";
                state.captureRequiresStart = true;
                return structuredClone(state);
              }
              state.capturing = ["screen", "music"].includes(state.settings.mode);
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
    });
    await page.goto(`http://127.0.0.1:${server.address().port}`);
    await page.waitForFunction(
      () => typeof settings !== "undefined" && settings,
    );
    assert.match(await page.locator("#frame-rate").innerText(), /Preview only/);
    assert.equal(
      await page.evaluate(() =>
        fixture.calls.some((a) => ["connect", "startCapture", "start"].includes(a.type)),
      ),
      false,
      "Startup never captures implicitly",
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
      edits.filter((a) => a.type === "previewSettings").length <= 3,
      "Drag updates coalesced",
    );
    assert.equal(
      edits.filter((a) => a.type === "settings").length,
      1,
      "One persistence write",
    );
    assert.equal(edits.at(-1).settings.brightness, 100);
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
    for (const width of [1440, 1180, 880, 650, 540, 390]) {
      await page.setViewportSize({ width, height: 900 });
      assert(
        await page.evaluate(
          () => document.documentElement.scrollWidth <= innerWidth,
        ),
        `No horizontal overflow at ${width}px`,
      );
      if (width <= 760) {
        const controls = await page.locator(".tuning").boundingBox();
        const preview = await page.locator(".preview-card").boundingBox();
        assert(
          controls.y < preview.y,
          "Compact layout puts controls before output preview",
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
    await page.locator(".telemetry summary").click();
    assert.equal(await page.locator("#usb-fps").innerText(), "29");
    assert.equal(await page.locator("#audio-level").innerText(), "25% RMS");
    assert.equal(await page.locator("#write-errors").innerText(), "2");
    await page.screenshot({
      path: ".local/ui-music-fixture.png",
      fullPage: true,
    });
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
    await page.locator("#setup summary").click();
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
    await page.evaluate(() => { fixture.state.pauseReason = ""; fixture.emit(); });
    assert.equal(await page.evaluate(() => fixture.state.requested), false);
    // Preference edits never start lighting, and login registration failures roll back.
    await page.locator("#restore-on-start").check();
    await page.locator("#resume-capture").check();
    await page.waitForFunction(() => document.getElementById("save-state").textContent === "Saved");
    assert.equal(await page.evaluate(() => fixture.state.requested), false);
    assert.equal(await page.evaluate(() => fixture.state.settings.resumeCapture), true);
    await page.locator("#launch-at-login").check();
    await page.waitForFunction(() => !busy);
    assert.equal(await page.evaluate(() => fixture.login), true);
    await page.evaluate(() => { fixture.failLogin = true; });
    await page.locator("#launch-at-login").click();
    await page.waitForFunction(() => !busy);
    assert.equal(await page.locator("#launch-at-login").isChecked(), true);
    assert.match(await page.locator("#notice").innerText(), /Test login registration failed/);
    for (const width of [1180, 650, 390]) {
      await page.setViewportSize({ width, height: 900 });
      assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), `Behavior settings fit at ${width}px`);
    }
    await page.setViewportSize({ width: 1180, height: 1000 });
    await page.screenshot({ path: ".local/ui-lifecycle.png", fullPage: true });
    assert.deepEqual(errors, []);
    console.log(
      "UI passed: drag coalescing, persistence, 5 effects, 4 music styles, telemetry, capture recovery, 6 responsive widths, CSP and errors.",
    );
  } finally {
    await browser?.close();
    await new Promise((resolve) => server.close(resolve));
  }
})().catch((e) => {
  console.error(e);
  process.exitCode = 1;
});
