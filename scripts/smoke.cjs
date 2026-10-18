// Development smoke test. Does not connect to the strip or capture the desktop.
const { _electron: electron } = require(process.env.PLAYWRIGHT_PATH || 'playwright');
const path = require('node:path');
const fs = require('node:fs');
(async () => {
  const env = { ...process.env }; delete env.ELECTRON_RUN_AS_NODE;
  const instance = await electron.launch({ executablePath: require('electron'), args: [path.resolve('.')], env });
  try {
    const page = await instance.firstWindow();
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
    await page.waitForFunction(() => document.querySelector('#status-label').textContent === 'Not connected');
    await page.getByRole('button', { name: 'Music Feel every beat' }).click();
    await page.getByRole('button', { name: 'Start capture' }).waitFor();
    await page.getByRole('button', { name: 'Solid color Set the mood' }).click();
    await page.getByRole('button', { name: 'Violet', exact: true }).click();
    await page.getByLabel('Brightness', { exact: false }).fill('38');
    await page.waitForTimeout(500);
    fs.mkdirSync('.local', { recursive: true });
    await page.screenshot({ path: '.local/studio.png' });
    console.log(JSON.stringify({ title: await page.title(), errors, brightness: await page.locator('#brightness-value').textContent(), screenshot: '.local/studio.png' }));
    if (errors.length) process.exitCode = 1;
  } finally { await instance.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
