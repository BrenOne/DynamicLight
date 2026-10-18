// Explicit hardware integration test: changes lighting and briefly captures screen/audio.
const { _electron: electron } = require(process.env.PLAYWRIGHT_PATH || 'playwright');
const path = require('node:path');
(async () => {
  const env = { ...process.env }; delete env.ELECTRON_RUN_AS_NODE;
  const instance = await electron.launch({ executablePath: process.env.DYNAMICLIGHT_EXECUTABLE || require('electron'), args: process.env.DYNAMICLIGHT_EXECUTABLE ? [] : [path.resolve('.')], env });
  try {
    const page = await instance.firstWindow(); const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.locator('#connect').click();
    await page.waitForFunction(() => document.querySelector('#status-label').textContent === 'Connected');
    await page.getByRole('button', { name: 'Screen sync Beyond the edges' }).click();
    await page.locator('#capture').click();
    await page.waitForFunction(() => document.querySelector('#capture-video').videoWidth > 0, { timeout: 15000 });
    await page.waitForTimeout(1800);
    const screen = await page.evaluate(() => ({ width: video.videoWidth, height: video.videoHeight, fps: document.querySelector('#frame-rate').textContent, notice: document.querySelector('#notice').textContent, track: stream?.getVideoTracks()[0]?.readyState }));
    await page.getByRole('button', { name: 'Music Feel every beat' }).click();
    await page.locator('#capture').click();
    await page.waitForTimeout(2000);
    const music = await page.evaluate(() => ({ track: stream?.getAudioTracks()[0]?.readyState, context: audio?.state, notice: document.querySelector('#notice').textContent }));
    await page.getByRole('button', { name: 'Color flow A moving spectrum' }).click();
    await page.waitForTimeout(1200);
    const rainbow = await page.locator('#frame-rate').textContent();
    await page.getByRole('button', { name: 'Solid color Set the mood' }).click();
    await page.getByRole('button', { name: 'Violet', exact: true }).click();
    await page.locator('#brightness').fill('38');
    await page.locator('#power').click();
    await page.waitForFunction(() => document.querySelector('#power').getAttribute('aria-pressed') === 'false');
    await page.locator('#power').click();
    await page.waitForTimeout(1200);
    await page.screenshot({ path: '.local/studio-connected.png' });
    console.log(JSON.stringify({ screen, music, rainbow, errors }));
    if (screen.track !== 'live' || music.track !== 'live' || errors.length) process.exitCode = 1;
  } finally { await instance.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
