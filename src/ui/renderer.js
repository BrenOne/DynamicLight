'use strict';
const $ = id => document.getElementById(id);
const { invoke } = window.__TAURI__.core;
let settings, runtime, saveTimer, busy = false, queue = Promise.resolve(), platform = '';
const labels = { solid: 'Solid color', screen: 'Screen sync', music: 'Music', rainbow: 'Color flow' };
const captions = { solid: 'A steady glow, exactly how you like it.', screen: 'Your screen, extended into your space.', music: 'A little rhythm for your room.', rainbow: 'Let the colors take their time.' };
function notice(message, error = false) {
  $('notice').textContent = message; $('notice').hidden = !message;
  $('notice').classList.toggle('error', error);
}
function request(action) {
  // Keep slider saves ordered with power and capture commands.
  const pending = queue.catch(() => {}).then(() => invoke('control', { action }));
  queue = pending;
  return pending.then(value => { renderRuntime(value); return value; });
}
function renderRuntime(value) {
  runtime = value;
  if (!settings) return;
  const connected = value.status.connected;
  $('status-dot').classList.toggle('connected', connected);
  $('status-label').textContent = connected ? 'Connected' : 'Not connected';
  $('connect').textContent = connected ? 'Disconnect strip ↗' : 'Connect strip ↗';
  $('connection-message').textContent = value.status.message;
  $('power').classList.toggle('off', !value.powered);
  $('power').setAttribute('aria-pressed', value.powered);
  const waiting = ['screen', 'music'].includes(settings.mode) && !value.capturing;
  $('power').querySelector('span').textContent = !value.powered ? 'Turn lights on' : !connected || waiting ? 'Power enabled' : 'Turn lights off';
  $('power').setAttribute('aria-label', value.powered ? 'Turn lights off' : 'Turn lights on');
  $('capture').textContent = value.capturing ? 'Stop capture' : connected ? 'Start capture' : 'Connect & start capture';
  $('capture').disabled = busy;
  $('frame-rate').textContent = !connected ? 'Disconnected · Preview only' : !value.powered ? 'Lights off' : waiting ? 'Capture stopped · Strip dark' : settings.brightness === 0 ? 'Brightness is zero · Strip dark' : value.capturing || settings.mode === 'rainbow' ? `${value.fps} FPS · USB` : 'Solid color · USB';
  if (value.error) notice(value.error, true);
  draw(value.colors);
}
function updateUI() {
  document.querySelectorAll('[data-mode]').forEach(button => {
    const selected = button.dataset.mode === settings.mode;
    button.classList.toggle('selected', selected); button.setAttribute('aria-pressed', selected);
  });
  $('mode-badge').textContent = labels[settings.mode]; $('preview-caption').textContent = captions[settings.mode];
  $('color').value = settings.color; $('hex-value').textContent = settings.color.toUpperCase();
  for (const name of ['brightness', 'sensitivity', 'smoothing']) {
    $(name).value = settings[name]; $(name + '-value').textContent = settings[name] + '%';
  }
  for (const name of ['count', 'sides', 'offset', 'fps']) $(name).value = settings[name];
  $('audio-source').value = settings.audioSource; $('reverse').checked = settings.reverse;
  $('off-on-sleep').checked = settings.offOnSleep; $('off-on-quit').checked = settings.offOnQuit;
  $('device-detail').textContent = `${settings.count} LEDs · USB HID`;
  $('capture-controls').hidden = !['screen', 'music'].includes(settings.mode);
  $('music-controls').hidden = settings.mode !== 'music';
  $('screen').disabled = settings.mode !== 'screen'; $('refresh-screens').disabled = busy || settings.mode !== 'screen';
  $('capture-help').textContent = settings.mode === 'screen'
    ? 'Capture stays on this computer. ' + (platform === 'linux' ? 'Screen sync needs an X11 session; Wayland support is planned.' : platform === 'macos' ? 'Allow Screen Recording in Privacy settings.' : 'The preview shows sampled edge colors.')
    : 'Audio is analyzed locally. ' + (platform === 'linux' ? 'Computer audio uses PulseAudio or PipeWire. Microphone uses your default input.' : platform === 'macos' ? 'Allow Screen & System Audio Recording for computer audio, or Microphone access for your mic.' : 'Computer audio follows your speakers. Microphone mode listens to your room.');
  if (runtime) renderRuntime(runtime);
}
function save() { clearTimeout(saveTimer); return request({ type: 'settings', settings: { ...settings } }); }
function changed() { updateUI(); clearTimeout(saveTimer); saveTimer = setTimeout(() => save().catch(error => notice(String(error), true)), 180); }
async function run(action) {
  if (busy) return;
  busy = true;
  const controls = ['connect', 'power', 'capture', 'refresh-screens'];
  controls.forEach(id => { $(id).disabled = true; });
  document.querySelectorAll('[data-mode]').forEach(button => { button.disabled = true; });
  notice('');
  try { await action(); } catch (error) { notice(String(error), true); }
  finally {
    busy = false; controls.forEach(id => { $(id).disabled = false; });
    document.querySelectorAll('[data-mode]').forEach(button => { button.disabled = false; }); updateUI();
  }
}
function positions(count, sides) {
  const aspect = 16 / 9, perimeter = 2 + aspect * (sides === 4 ? 2 : 1);
  return Array.from({ length: count }, (_, i) => {
    const p = (i + .5) / count * perimeter;
    if (p < 1) return [0, 1 - p]; if (p < 1 + aspect) return [(p - 1) / aspect, 0];
    if (p < 2 + aspect) return [1, p - 1 - aspect]; return [1 - (p - 2 - aspect) / aspect, 1];
  });
}
function draw(colors) {
  const preview = $('preview').getContext('2d'); preview.clearRect(0, 0, 1000, 300);
  const points = positions(colors.length, settings.sides);
  const shown = runtime.powered ? colors : colors.map(() => [28, 27, 35]);
  shown.forEach((rgb, i) => {
    const point = points[i]; preview.beginPath(); preview.shadowBlur = runtime.powered ? 28 : 0;
    preview.shadowColor = `rgb(${rgb.join(',')})`; preview.fillStyle = `rgb(${rgb.join(',')})`;
    preview.arc(268 + point[0] * 464, 28 + point[1] * 224, 4, 0, Math.PI * 2); preview.fill();
  });
  preview.shadowBlur = 0; preview.fillStyle = '#0f1016'; preview.strokeStyle = '#49434f'; preview.lineWidth = 2;
  preview.beginPath(); preview.roundRect(281, 40, 438, 200, 7); preview.fill(); preview.stroke();
  const gradient = preview.createLinearGradient(290, 50, 710, 235), color = shown[Math.floor(shown.length / 2)] || [60, 40, 90];
  gradient.addColorStop(0, `rgba(${color.join(',')},0.22)`); gradient.addColorStop(1, '#15131d');
  preview.fillStyle = gradient; preview.fillRect(289, 48, 422, 184);
  preview.fillStyle = '#c9bbdf'; preview.font = '18px Segoe UI'; preview.textAlign = 'center';
  preview.fillText(!runtime.powered ? 'LIGHTS OFF' : ['screen', 'music'].includes(settings.mode) && !runtime.capturing ? 'START CAPTURE TO LIGHT YOUR STRIP' : settings.mode === 'music' ? '♫' : settings.mode === 'screen' ? 'LIVE EDGE COLORS' : 'DYNAMIC / LIGHT', 500, 145);
  preview.fillStyle = '#3b3543'; preview.fillRect(489, 242, 22, 25);
  preview.beginPath(); preview.roundRect(451, 267, 98, 5, 3); preview.fill();
}
async function init() {
  platform = await invoke('platform_name');
  if (platform === 'linux') $('close-behavior').textContent = 'Closing the window minimizes it and keeps lighting running. Use Quit DynamicLight to exit.';
  await window.__TAURI__.event.listen('lighting', event => renderRuntime(event.payload));
  const initial = await request({ type: 'snapshot' }); settings = initial.settings; updateUI();
  $('connect').onclick = () => run(async () => { await save(); await request({ type: runtime.status.connected ? 'disconnect' : 'connect' }); });
  $('power').onclick = () => run(async () => { await save(); await request({ type: 'power', on: !runtime.powered }); });
  document.querySelectorAll('[data-mode]').forEach(button => {
    button.onclick = () => run(async () => { settings.mode = button.dataset.mode; await save(); if (['screen','music'].includes(settings.mode)) notice('Click Start capture to begin.'); });
  });
  document.querySelectorAll('[data-color]').forEach(button => { button.onclick = () => { settings.color = button.dataset.color; changed(); }; });
  $('color').oninput = event => { settings.color = event.target.value; changed(); };
  for (const name of ['brightness', 'sensitivity', 'smoothing']) $(name).oninput = event => { settings[name] = Number(event.target.value); changed(); };
  for (const name of ['count', 'sides', 'offset', 'fps']) $(name).onchange = event => { if (event.target.checkValidity()) { settings[name] = Number(event.target.value); changed(); } };
  for (const [id, key] of [['reverse','reverse'],['off-on-sleep','offOnSleep'],['off-on-quit','offOnQuit']]) $(id).onchange = event => { settings[key] = event.target.checked; changed(); };
  $('audio-source').onchange = event => { settings.audioSource = event.target.value; changed(); notice('Click Start capture to use the selected audio source.'); };
  $('screen').onchange = event => { settings.screenId = event.target.value; changed(); notice('Click Start capture to use the selected display.'); };
  $('refresh-screens').onclick = () => run(async () => {
    const screens = await invoke('displays'); $('screen').replaceChildren();
    for (const screen of screens) { const option = document.createElement('option'); option.value = screen.id; option.textContent = screen.name; $('screen').append(option); }
    if (screens.some(screen => screen.id === settings.screenId)) $('screen').value = settings.screenId;
    else { settings.screenId = screens[0]?.id || ''; await save(); }
  });
  $('capture').onclick = () => run(async () => {
    await save();
    if (runtime.capturing) { await request({ type: 'stopCapture' }); return; }
    if (!runtime.status.connected) await request({ type: 'connect' });
    if (!runtime.powered) await request({ type: 'power', on: true });
    await request({ type: 'startCapture' });
  });
  $('quit').onclick = () => run(async () => { await save(); await invoke('quit'); });
  window.addEventListener('focus', () => request({ type: 'snapshot' }).catch(error => notice(String(error), true)));
}
init().catch(error => notice(`Unable to initialize: ${error}`, true));
