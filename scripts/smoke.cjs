// Native Windows UI/IPC test. Only --hardware may connect USB or capture.
const assert = require("node:assert/strict");
const { spawn, execFileSync } = require("node:child_process");
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
    process.env.DYNAMICLIGHT_BACKGROUND_TEST ? ["--autostart"] : [],
    {
      env: {
        ...process.env,
        WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: "--remote-debugging-port=9223",
      },
      stdio: "ignore",
    },
  );
  let browser, page, original, originalProfiles, originalUpdates;
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
    const version = require("../package.json").version;
    originalUpdates = await page.evaluate(() => window.__TAURI__.core.invoke("update_status"));
    assert.equal(originalUpdates.currentVersion, version);
    await page.evaluate(() => window.__TAURI__.core.invoke("update_automatic", { enabled: false }));
    assert.equal((await page.evaluate(() => window.__TAURI__.core.invoke("update_status"))).automatic, false);
    assert.equal((await control({ type: "snapshot" })).status.connected, false, "Changing updater preferences never opens USB");
    assert.equal(
      await page.locator("[data-app-version]").innerText(),
      "v" + version,
    );
    for (const view of ["scenes", "device", "settings", "control"]) {
      await page.locator('.app-sidebar [data-view="' + view + '"]').click();
      assert.equal(
        (await control({ type: "snapshot" })).requested,
        false,
        "Browsing never starts native lighting",
      );
    }
    assert.equal((await control({ type: "snapshot" })).requested, false);
    assert.equal((await control({ type: "snapshot" })).powered, false);
    assert.equal(
      typeof (await page.evaluate(() =>
        window.__TAURI__.core.invoke("launch_at_login", { enabled: null }),
      )),
      "boolean",
    );
    // Read the real Windows registration result without changing the user's login settings.
    await delay(300);
    assert.equal((await control({ type: "snapshot" })).lifecycleWarning, "");
    for (
      let i = 0;
      i < 30 && !(await control({ type: "snapshot" })).lifecycleReady;
      i++
    )
      await delay(100);
    assert.equal(
      (await control({ type: "snapshot" })).lifecycleReady,
      true,
      "Native initial display/session state arrived",
    );
    assert.equal((await control({ type: "snapshot" })).capturing, false);
    assert.equal((await control({ type: "snapshot" })).status.connected, false);
    const diagnostic = await page.evaluate(() =>
      window.__TAURI__.core.invoke("diagnostic_report", {
        enabled: null,
        clear: false,
      }),
    );
    assert(diagnostic.text.includes("Missed frame slots:"));
    assert(!diagnostic.text.includes(process.env.USERPROFILE));
    await control({
      type: "settings",
      settings: { ...original, mode: "solid", brightness: 23 },
    });
    await control({ type: "selectMode", mode: "rainbow" });
    await control({
      type: "settings",
      settings: {
        ...(await control({ type: "snapshot" })).settings,
        brightness: 71,
        effectReverse: true,
      },
    });
    assert.equal(
      (await control({ type: "selectMode", mode: "solid" })).settings
        .brightness,
      23,
    );
    assert.equal((await control({ type: "snapshot" })).status.connected, false);
    await control({ type: "settings", settings: original });
    const starters = await page.evaluate(() =>
      window.__TAURI__.core.invoke("starter_scenes"),
    );
    assert.equal(starters.profiles.length, 4);
    const previews = await page.evaluate(
      (settings) =>
        window.__TAURI__.core.invoke("scene_previews", {
          settings,
          seconds: 2,
        }),
      starters.profiles.map((p) => p.settings),
    );
    assert.equal(previews.length, 4);
    assert(previews.every((frame) => frame.length === 48));
    assert(
      await page
        .locator(".app-sidebar .icon")
        .first()
        .evaluate((svg) => svg.getBBox().width > 0),
      "Local SVG symbols load under the native CSP",
    );
    await page.emulateMedia({ colorScheme: "dark" });
    await page.evaluate(async () => {
      settings = (
        await window.__TAURI__.core.invoke("control", {
          action: { type: "selectMode", mode: "rainbow" },
        })
      ).settings;
      updateUI();
    });
    await page.waitForTimeout(350);
    assert.equal(
      (await control({ type: "snapshot" })).status.connected,
      false,
      "Native preview does not open USB",
    );
    await page.screenshot({ path: ".local/native-workspace.png" });
    await page.evaluate(async (original) => {
      settings = (
        await window.__TAURI__.core.invoke("control", {
          action: { type: "settings", settings: original },
        })
      ).settings;
      updateUI();
    }, original);
    await page.evaluate(() =>
      window.__TAURI__.core.invoke("compact_window", {
        operation: "open",
        pinned: null,
      }),
    );
    let compactPage;
    for (let i = 0; i < 40; i++) {
      compactPage = browser
        .contexts()[0]
        .pages()
        .find((p) => p.url().includes("compact.html"));
      if (compactPage) break;
      await delay(100);
    }
    assert(compactPage, "Native compact webview opened");
    compactPage.on("pageerror", (e) => errors.push(e.message));
    await compactPage.emulateMedia({ colorScheme: "dark" });
    await compactPage.waitForFunction(
      () => !document.querySelector("#quick-power").disabled,
    );
    if (process.platform === "win32") {
      console.log(
        execFileSync(
          "powershell.exe",
          [
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            path.resolve("scripts/windows-icons.ps1"),
            "-ProcessId",
            String(child.pid),
          ],
          { encoding: "utf8", timeout: 15000 },
        ).trim(),
      );
      await page.waitForTimeout(200);
      assert(
        await page
          .locator(".app-shell")
          .evaluate((el) => el.getBoundingClientRect().width <= 1764),
        "Native maximized window keeps controls within a usable work area",
      );
      await page.screenshot({ path: ".local/native-maximized.png" });
    }
    assert.equal(
      await compactPage.locator("#quick-error").innerText(),
      "",
      "Compact event subscriptions are permitted",
    );
    assert.equal(
      await compactPage.locator("[data-app-version]").innerText(),
      "v" + version,
    );
    await compactPage.locator("#quick-brightness").fill("42");
    await compactPage.locator("#quick-brightness").dispatchEvent("input");
    await page.waitForFunction(() => settings.brightness === 42);
    assert.equal((await control({ type: "snapshot" })).status.connected, false);
    await compactPage.locator("#quick-pin").check();
    await compactPage.waitForFunction(() => !busy);
    assert.equal(
      await compactPage.evaluate(() =>
        window.__TAURI__.core.invoke("compact_window", {
          operation: "get",
          pinned: null,
        }),
      ),
      true,
    );
    await compactPage.locator("#quick-pin").uncheck();
    await compactPage.waitForFunction(() => !busy);
    await compactPage.screenshot({ path: ".local/native-compact.png" });
    const quickGeometry = await compactPage.evaluate(() => ({
      modes: document.querySelector(".quick-modes").getBoundingClientRect().top,
      brightness: document
        .querySelector(".quick-brightness")
        .getBoundingClientRect().top,
      footer: document.querySelector(".quick-footer").getBoundingClientRect()
        .bottom,
      height: innerHeight,
    }));
    assert(
      quickGeometry.modes < quickGeometry.brightness,
      "Native mode buttons precede brightness",
    );
    assert(
      quickGeometry.footer <= quickGeometry.height + 1,
      "Native Quick-controls footer fits",
    );
    await compactPage.locator("#open-studio").click();
    await compactPage.waitForFunction(() => !busy);
    await compactPage.evaluate(() =>
      window.__TAURI__.core.invoke("compact_window", {
        operation: "hide",
        pinned: null,
      }),
    );
    await page.locator('.app-sidebar [data-view="scenes"]').click();
    await page.waitForTimeout(500);
    await page.screenshot({ path: ".local/native-scenes.png", fullPage: true });
    await page.locator('.app-sidebar [data-view="control"]').click();
    await control({ type: "settings", settings: original });
    await assert.rejects(
      control({ type: "settings", settings: { ...original, count: 128 } }),
    );
    await assert.rejects(control({ type: "suspend" }));
    const frames = await page.evaluate(() =>
      window.__TAURI__.core.invoke("effect_previews", { settings, seconds: 2 }),
    );
    originalProfiles = await page.evaluate(() =>
      window.__TAURI__.core.invoke("profile_library", { library: null }),
    );
    const profile = {
      id: "native-test",
      name: "Native test",
      favorite: true,
      shortcut: "Control+Alt+Shift+F12",
      settings: original,
    };
    const saved = await page.evaluate(
      (p) =>
        window.__TAURI__.core.invoke("profile_library", {
          library: { profiles: [p] },
        }),
      profile,
    );
    assert.equal(saved.profiles[0].name, "Native test");
    assert.equal(saved.profiles[0].source.screenId, original.screenId);
    for (const key of [
      "screenId",
      "count",
      "resumeCapture",
      "maxBrightness",
      "modeSettings",
    ])
      assert.equal(key in saved.profiles[0].settings, false);
    const roundtrip = await page.evaluate(
      (library) => window.__TAURI__.core.invoke("profile_library", { library }),
      saved,
    );
    assert.deepEqual(
      roundtrip,
      saved,
      "Separated scene and local source survive native persistence",
    );
    await compactPage.waitForFunction(
      () => document.querySelectorAll("#quick-favorites button").length === 1,
    );
    const importedBundle = {
      format: "dynamiclight",
      version: 1,
      profiles: [
        {
          id: "native-test",
          name: "Native test",
          settings: saved.profiles[0].settings,
        },
      ],
      palettes: [],
    };
    const importPreview = await page.evaluate(
      (text) =>
        window.__TAURI__.core.invoke("preview_profile_import", { text }),
      JSON.stringify(importedBundle),
    );
    assert.equal(importPreview.conflicts[0], "Native test");
    const imported = await page.evaluate(
      (p) =>
        window.__TAURI__.core.invoke("import_profiles", {
          bundle: p.bundle,
          expected: p.base,
          policy: "copy",
        }),
      importPreview,
    );
    assert.equal(imported.profiles.length, 2);
    assert.equal(imported.profiles[1].shortcut, "");
    assert.equal(imported.profiles[1].source.screenId, "");
    await assert.rejects(
      page.evaluate(
        (p) =>
          window.__TAURI__.core.invoke("import_profiles", {
            bundle: p.bundle,
            expected: p.base,
            policy: "replace",
          }),
        importPreview,
      ),
      /library changed/,
    );
    assert.equal(
      (await control({ type: "snapshot" })).status.connected,
      false,
      "Import never connects USB",
    );
    await page.evaluate(
      (library) => window.__TAURI__.core.invoke("profile_library", { library }),
      saved,
    );
    await assert.rejects(
      page.evaluate(
        (p) =>
          window.__TAURI__.core.invoke("profile_library", {
            library: { profiles: [p, { ...p, id: "duplicate" }] },
          }),
        profile,
      ),
    );
    assert.equal(
      (
        await page.evaluate(() =>
          window.__TAURI__.core.invoke("profile_library", { library: null }),
        )
      ).profiles.length,
      1,
    );
    await page.evaluate(
      (library) => window.__TAURI__.core.invoke("profile_library", { library }),
      originalProfiles,
    );
    assert.equal(
      (await control({ type: "snapshot" })).status.connected,
      false,
      "Profile edits never connect USB",
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
        markSettings(["mode", "effect", "brightness", "smoothing"]);
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
        if (originalUpdates)
          await page.evaluate((enabled) => window.__TAURI__.core.invoke("update_automatic", { enabled }), originalUpdates.automatic);
        if (originalProfiles)
          await page.evaluate(
            (library) =>
              window.__TAURI__.core.invoke("profile_library", { library }),
            originalProfiles,
          );
        if (original)
          await page.evaluate(async (s) => {
            settings = s;
            markSettings(
              Object.keys(s).filter((key) => key !== "modeSettings"),
            );
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
