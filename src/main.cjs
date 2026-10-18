'use strict';
const { app, BrowserWindow, ipcMain, desktopCapturer, session, systemPreferences, powerMonitor, Tray, Menu } = require('electron');
const fs = require('node:fs');
const path = require('node:path');
const { pathToFileURL } = require('node:url');
const { Controller } = require('./controller.cjs');
const { defaults, validate } = require('./settings.cjs');
const { checkRGB } = require('./protocol.cjs');

let win, tray, controller, settings = { ...defaults }, settingsFile;
let desiredConnection = false, quitting = false, suspended = false, captureRequest = null;
const entry = path.join(__dirname, 'ui', 'index.html');
const entryURL = pathToFileURL(entry).href;
if (!app.requestSingleInstanceLock()) app.quit();
else {
  app.on('second-instance', () => { win?.show(); win?.focus(); });
  app.whenReady().then(start).catch(error => { console.error(error); app.quit(); });
}
function send(event, data) { if (win && !win.isDestroyed()) win.webContents.send(event, data); }
async function turnOffWhenIdle() {
  for (let n = 0; controller.busy && n < 40; n++) await new Promise(resolve => setTimeout(resolve, 25));
  return controller.solid([0, 0, 0], settings.count);
}
function handle(name, fn) {
  ipcMain.handle(name, async (event, ...args) => {
    if (event.sender !== win?.webContents || event.senderFrame?.url !== entryURL) throw new Error('Untrusted request');
    return fn(...args);
  });
}
async function start() {
  settingsFile = path.join(app.getPath('userData'), 'settings.json');
  try { settings = validate(JSON.parse(fs.readFileSync(settingsFile, 'utf8'))); } catch {}
  controller = new Controller(require('node-hid'));
  controller.on('status', status => send('status', status));
  win = new BrowserWindow({
    width: 1180, height: 830, minWidth: 900, minHeight: 720,
    title: 'DynamicLight', backgroundColor: '#101115', autoHideMenuBar: true,
    icon: path.join(__dirname, '../assets/icon.png'),
    webPreferences: { preload: path.join(__dirname, 'preload.cjs'), contextIsolation: true, nodeIntegration: false, sandbox: true, backgroundThrottling: false }
  });
  win.webContents.setWindowOpenHandler(() => ({ action: 'deny' }));
  win.webContents.on('will-navigate', event => event.preventDefault());
  win.on('close', event => { if (!quitting && tray) { event.preventDefault(); win.hide(); } });
  session.defaultSession.setPermissionRequestHandler((contents, permission, callback) => {
    callback(contents === win.webContents && ['media', 'display-capture'].includes(permission));
  });
  session.defaultSession.setDisplayMediaRequestHandler(async (request, callback) => {
    const options = captureRequest; captureRequest = null;
    if (!options || request.frame !== win.webContents.mainFrame) return callback({});
    try {
      const sources = await desktopCapturer.getSources({ types: ['screen'], thumbnailSize: { width: 0, height: 0 } });
      const selected = sources.find(source => source.id === options.screenId) || sources[0];
      if (!selected) return callback({});
      callback(options.audio ? { video: selected, audio: 'loopback' } : { video: selected });
    } catch { callback({}); }
  });
  handle('init', () => ({ settings, status: controller.status, platform: process.platform }));
  handle('connect', async () => { desiredConnection = true; return controller.connect(); });
  handle('disconnect', async () => { desiredConnection = false; await controller.disconnect(); return controller.status; });
  handle('settings', input => {
    const next = validate(input);
    const temp = settingsFile + '.tmp';
    fs.writeFileSync(temp, JSON.stringify(next, null, 2)); fs.renameSync(temp, settingsFile);
    settings = next; return settings;
  });
  handle('solid', rgb => { checkRGB(rgb); return !suspended && controller.solid(rgb, settings.count); });
  handle('frame', colors => {
    if (!Array.isArray(colors) || colors.length !== settings.count) throw new Error('Wrong frame size');
    colors.forEach(checkRGB);
    return !suspended && controller.frame(colors);
  });
  handle('screens', async () => {
    const screens = await desktopCapturer.getSources({ types: ['screen'], thumbnailSize: { width: 0, height: 0 } });
    return screens.map(({ id, name }) => ({ id, name }));
  });
  handle('prepare-capture', async options => {
    if (!options || typeof options.audio !== 'boolean' || typeof options.screenId !== 'string') throw new Error('Invalid capture request');
    if (process.platform === 'darwin' && systemPreferences.getMediaAccessStatus('screen') === 'denied') {
      throw new Error('Enable DynamicLight in System Settings → Privacy & Security → Screen & System Audio Recording, then restart the app.');
    }
    captureRequest = { audio: options.audio, screenId: options.screenId };
    return true;
  });
  tray = new Tray(path.join(__dirname, '../assets/icon.png'));
  tray.setToolTip('DynamicLight');
  tray.setContextMenu(Menu.buildFromTemplate([
    { label: 'Open DynamicLight', click: () => win.show() },
    { label: 'Turn lights off', click: () => { send('pause', { turnOff: true }); } },
    { type: 'separator' }, { label: 'Quit', click: () => app.quit() }
  ]));
  tray.on('click', () => win.show());
  powerMonitor.on('suspend', async () => {
    suspended = true; send('pause');
    if (settings.offOnSleep) await turnOffWhenIdle();
    await controller.disconnect('Computer sleeping');
  });
  powerMonitor.on('resume', () => { suspended = false; send('resume'); });
  setInterval(() => { if (desiredConnection && !suspended && !controller.device) void controller.connect(); }, 3000).unref();
  await win.loadFile(entry);
}
app.on('activate', () => win?.show());
app.on('before-quit', event => {
  if (quitting || !controller) return;
  event.preventDefault(); quitting = true; desiredConnection = false; send('pause');
  const shutdown = async () => {
    // Let an in-flight report finish before sending the final black frame.
    if (settings.offOnQuit) await turnOffWhenIdle();
    await controller.disconnect(); app.quit();
  };
  void shutdown();
});
