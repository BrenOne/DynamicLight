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
    original;
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
    console.log(
      "Native studio passed: bounded frames, invalid effect rejection, editor Apply while off, native persistence, cross-window appearance; no USB/capture.",
    );
  } finally {
    if (page && isolated) {
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
