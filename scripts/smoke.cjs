// Native Windows UI/IPC test. Only --hardware may connect USB or capture.
const assert = require("node:assert/strict");
const { spawn } = require("node:child_process");
const { once } = require("node:events");
const path = require("node:path");
const fs = require("node:fs");
const { chromium } = require(process.env.PLAYWRIGHT_PATH || "playwright");
const hardware = process.argv.includes("--hardware");
const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
(async () => {
  const child = spawn(
    process.env.DYNAMICLIGHT_EXECUTABLE ||
      path.resolve("src-tauri/target/release/dynamiclight.exe"),
    [],
    {
      env: {
        ...process.env,
        WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: "--remote-debugging-port=9223",
      },
      stdio: "ignore",
    },
  );
  let browser, page, original;
  const errors = [];
  try {
    for (let i = 0; i < 60; i++) {
      if (child.exitCode !== null)
        throw Error(
          "App exited; close another instance or use a separate test identifier.",
        );
      try {
        browser = await chromium.connectOverCDP("http://127.0.0.1:9223");
        break;
      } catch {
        await delay(500);
      }
    }
    assert(browser);
    for (let i = 0; i < 40; i++) {
      page = browser
        .contexts()[0]
        ?.pages()
        .find((p) => p.url().includes("tauri.localhost"));
      if (page) break;
      await delay(250);
    }
    assert(page);
    page.on("pageerror", (e) => errors.push(e.message));
    await page.waitForFunction(
      () => typeof settings !== "undefined" && settings,
    );
    const control = (action) =>
      page.evaluate(
        (action) => window.__TAURI__.core.invoke("control", { action }),
        action,
      );
    original = (await control({ type: "snapshot" })).settings;
    assert.equal((await control({ type: "snapshot" })).capturing, false);
    assert.equal((await control({ type: "snapshot" })).status.connected, false);
    await assert.rejects(
      control({ type: "settings", settings: { ...original, count: 128 } }),
    );
    await assert.rejects(control({ type: "suspend" }));
    const frames = await page.evaluate(() =>
      window.__TAURI__.core.invoke("effect_previews", { settings, seconds: 2 }),
    );
    assert.equal(frames.length, 5);
    assert(frames.every((f) => f.length === 48));
    const later = await page.evaluate(() =>
      window.__TAURI__.core.invoke("effect_previews", { settings, seconds: 5 }),
    );
    assert.notDeepEqual(frames, later, "Thumbnails reflect native animation");
    if (hardware) {
      // No Connect/Power/Capture sequence: selecting the mode must do it all.
      for (const mode of ["solid", "screen", "music", "rainbow"]) {
        await page.locator(`[data-mode="${mode}"]`).click();
        await page.waitForFunction(() => !busy);
        const s = await control({ type: "snapshot" });
        assert(s.status.connected && s.powered);
        assert.equal(s.capturing, ["screen", "music"].includes(mode));
        assert.equal(s.error, "");
        if (mode === "screen") {
          await delay(1500);
          assert((await control({ type: "snapshot" })).captureSize[0] > 0);
        }
        if (mode === "music") {
          await delay(1500);
          assert((await control({ type: "snapshot" })).audioSamples > 0);
        }
      }
      const before = await control({ type: "snapshot" });
      const session = await page.context().newCDPSession(page);
      await session.send("Page.setWebLifecycleState", { state: "frozen" });
      await delay(2000);
      await session.send("Page.setWebLifecycleState", { state: "active" });
      assert(
        (await control({ type: "snapshot" })).framesWritten -
          before.framesWritten >
          20,
      );
      await page.locator("#power").click();
      await page.waitForFunction(() => !busy);
      assert.equal((await control({ type: "snapshot" })).powered, false);
    } else {
      // Change settings directly for a disconnected preview. Clicking a mode starts hardware now.
      await page.evaluate(async () => {
        settings.mode = "rainbow";
        settings.effect = "aurora";
        settings.brightness = 80;
        settings.smoothing = 40;
        await save();
        updateUI();
      });
      await page.setViewportSize({ width: 1280, height: 950 });
      await delay(1200);
      await page.evaluate(() => request({ type: "snapshot" }));
      const s = await control({ type: "snapshot" });
      assert.equal(s.status.connected, false);
      assert.equal(s.framesWritten, 0);
      assert.equal(s.lastWriteMs, null);
      assert.equal(await page.locator(".effect-art").count(), 5);
      assert(
        await page
          .locator(".brand img")
          .evaluate((img) => img.complete && img.naturalWidth > 0),
        "Shared SVG icon loaded",
      );
      fs.mkdirSync(".local", { recursive: true });
      await page.screenshot({
        path: ".local/native-effects.png",
        fullPage: true,
      });
      await page.emulateMedia({ reducedMotion: "reduce" });
      await delay(250);
      const first = await page
        .locator(".effect-art")
        .first()
        .evaluate((c) => c.toDataURL());
      await delay(350);
      assert.equal(
        await page
          .locator(".effect-art")
          .first()
          .evaluate((c) => c.toDataURL()),
        first,
        "Reduced-motion native previews freeze",
      );
    }
    assert.deepEqual(errors, []);
    console.log(
      "Native UI, icon, effect preview IPC, startup and reduced-motion checks passed" +
        (hardware ? " with hardware." : " without USB/capture."),
    );
  } finally {
    if (page && !page.isClosed()) {
      try {
        if (original)
          await page.evaluate(async (s) => {
            settings = s;
            await save();
          }, original);
        await page.evaluate(() => window.__TAURI__.core.invoke("quit"));
      } catch {}
    }
    if (child.exitCode === null)
      await Promise.race([once(child, "exit"), delay(5000)]);
    if (child.exitCode === null) {
      child.kill();
      throw Error("App did not exit cleanly");
    }
    await browser?.close();
  }
})().catch((e) => {
  console.error(e);
  process.exitCode = 1;
});
