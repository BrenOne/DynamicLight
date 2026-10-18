// Windows WebView2 integration test. --hardware explicitly enables USB and capture.
// Run against a built app, with no other DynamicLight instance running.
const assert = require('node:assert/strict');
const { spawn } = require('node:child_process');
const { once } = require('node:events');
const path = require('node:path');
const fs = require('node:fs');
const { chromium } = require(process.env.PLAYWRIGHT_PATH || 'playwright-core');
const hardware = process.argv.includes('--hardware');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
(async () => {
  const executable = process.env.DYNAMICLIGHT_EXECUTABLE || path.resolve('src-tauri/target/release/dynamiclight.exe');
  const port = Number(process.env.DYNAMICLIGHT_TEST_PORT || 9223);
  const child = spawn(executable, [], { env: { ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}` }, stdio: 'ignore' });
  let browser, page, original;
  const errors = [];
  try {
    for (let i = 0; i < 60; i++) {
      if (child.exitCode !== null) throw new Error('App exited; another instance may already be running.');
      try { browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`); break; } catch { await delay(500); }
    }
    assert(browser, 'WebView2 debugging connection');
    for (let i = 0; i < 40; i++) { page = browser.contexts()[0]?.pages().find(p => p.url().includes('tauri.localhost')); if (page) break; await delay(250); }
    assert(page, 'Main WebView');
    page.on('pageerror', e => errors.push(e.message));
    await page.waitForFunction(() => typeof settings !== 'undefined' && settings);
    const control = action => page.evaluate(action => window.__TAURI__.core.invoke('control', { action }), action);
    original = (await control({ type: 'snapshot' })).settings;
    assert.equal((await control({ type: 'snapshot' })).capturing, false);
    await assert.rejects(control({ type: 'settings', settings: { ...original, count: 128 } }));
    await assert.rejects(control({ type: 'suspend' }));
    await page.locator('[data-mode="solid"]').click();
    await page.locator('[data-color="#9b72ff"]').click();
    await page.locator('#brightness').fill('38'); await delay(350);
    await page.waitForFunction(() => !busy);
    if (hardware) {
      await page.locator('#connect').click();
      await page.waitForFunction(() => runtime.status.connected, { timeout: 15000 });
      await page.waitForFunction(() => !busy);
      await page.locator('[data-mode="screen"]').click();
      await page.waitForFunction(() => !busy);
      assert.match(await page.locator('#frame-rate').innerText(), /Capture stopped/);
      assert.equal(await page.locator('#power span').innerText(), 'Power enabled');
      await page.locator('#refresh-screens').click();
      await page.waitForFunction(() => !busy);
      // Starting capture must recover from both disconnected and power-off,
      // without requiring separate controls in another part of the window.
      await page.locator('#connect').click();
      await page.waitForFunction(() => !runtime.status.connected && !busy);
      await page.locator('#power').click();
      await page.waitForFunction(() => !runtime.powered && !busy);
      assert.equal(await page.locator('#capture').isEnabled(), true);
      await page.locator('#capture').click();
      await page.waitForFunction(() => runtime.capturing && runtime.captureSize[0] > 0, { timeout: 15000 });
      assert.equal((await control({ type: 'snapshot' })).powered, true);
      // Exclude display enumeration / capture startup from the FPS window.
      await delay(3000);
      let state = await control({ type: 'snapshot' });
      console.log(JSON.stringify({ screen: { dimensions: state.captureSize, fps: state.fps, connected: state.status.connected } }));
      assert(state.fps > 0 && state.status.connected);
      await page.waitForFunction(() => !busy);
      await page.locator('[data-mode="music"]').click();
      await page.waitForFunction(() => !busy);
      await page.locator('#audio-source').selectOption('system');
      await page.locator('#capture').click();
      await page.waitForFunction(() => runtime.capturing);
      await delay(1500); state = await control({ type: 'snapshot' });
      assert.equal(state.error, ''); assert(state.status.connected); assert(state.audioSamples > 0, 'System audio samples arrived');
      console.log(JSON.stringify({ audio: { capturing: state.capturing, samples: state.audioSamples, fps: state.fps } }));
      await page.waitForFunction(() => !busy);
      await page.locator('[data-mode="rainbow"]').click();
      await page.waitForFunction(() => !busy);
      const before = await control({ type: 'snapshot' });
      const session = await page.context().newCDPSession(page);
      await session.send('Page.setWebLifecycleState', { state: 'frozen' }); await delay(2000);
      await session.send('Page.setWebLifecycleState', { state: 'active' });
      const after = await control({ type: 'snapshot' });
      assert(after.framesWritten - before.framesWritten > 20, 'Native lighting continues with frozen renderer');
      console.log(JSON.stringify({ backgroundFrames: after.framesWritten - before.framesWritten, fps: after.fps }));
      await page.locator('#power').click(); await page.waitForFunction(() => !runtime.powered && !busy);
      assert.equal((await control({ type: 'snapshot' })).capturing, false);
      await page.locator('#power').click(); await page.waitForFunction(() => runtime.powered && !busy);
    }
    await page.locator('[data-mode="solid"]').click(); await page.waitForFunction(() => !busy);
    fs.mkdirSync('.local', { recursive: true });
    await page.screenshot({ path: '.local/tauri-studio.png' });
    assert.deepEqual(errors, []); console.log('UI and IPC checks passed');
  } finally {
    if (page && !page.isClosed()) {
      try {
        if (original) await page.evaluate(settings => window.__TAURI__.core.invoke('control', { action: { type: 'settings', settings } }), original);
        await page.evaluate(() => window.__TAURI__.core.invoke('quit'));
      } catch {}
    }
    if (child.exitCode === null) await Promise.race([once(child, 'exit'), delay(5000)]);
    if (child.exitCode === null) { child.kill(); throw new Error('App did not exit cleanly'); }
    await browser?.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
