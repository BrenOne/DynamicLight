'use strict';
const $ = id => document.getElementById(id);
const L = window.Lighting;
let settings, platform, connected = false, powered = true, switching = false;
let stream = null, audio = null, analyser = null, audioData = null;
let previous = null, lastSolid = '', lastColors = [], sending = false;
let saveTimer, timer, frameCounter = 0, frameTime = performance.now();
let captureGeneration = 0;
const canvas = document.createElement('canvas');
const ctx = canvas.getContext('2d', { willReadFrequently: true });
const video = $('capture-video');
const preview = $('preview').getContext('2d');
const labels = { solid: 'Solid color', screen: 'Screen sync', music: 'Music', rainbow: 'Color flow' };
const captions = { solid: 'A steady glow, exactly how you like it.', screen: 'Your screen, extended into your space.', music: 'A little rhythm for your room.', rainbow: 'Let the colors take their time.' };

function notice(message, error = false) {
  $('notice').textContent = message; $('notice').hidden = !message;
  $('notice').classList.toggle('error', error);
}
function status(value) {
  connected = value.connected; lastSolid = '';
  $('status-dot').classList.toggle('connected', connected);
  $('status-label').textContent = connected ? 'Connected' : 'Not connected';
  $('connect').textContent = connected ? 'Disconnect strip ↗' : 'Connect strip ↗';
  $('connection-message').textContent = value.message;
  if (connected && !powered) void black();
  if (!connected && value.message !== 'Ready to connect') notice(value.message, true);
  if (connected) notice('');
}
function updateUI() {
  document.querySelectorAll('[data-mode]').forEach(button => {
    const selected = button.dataset.mode === settings.mode;
    button.classList.toggle('selected', selected); button.setAttribute('aria-pressed', selected);
  });
  $('mode-badge').textContent = labels[settings.mode];
  $('preview-caption').textContent = captions[settings.mode];
  $('color').value = settings.color; $('hex-value').textContent = settings.color.toUpperCase();
  for (const name of ['brightness', 'sensitivity', 'smoothing']) {
    $(name).value = settings[name]; $(name + '-value').textContent = settings[name] + '%';
  }
  for (const name of ['count', 'sides', 'offset', 'fps']) $(name).value = settings[name];
  $('audio-source').value = settings.audioSource;
  $('reverse').checked = settings.reverse;
  $('off-on-sleep').checked = settings.offOnSleep; $('off-on-quit').checked = settings.offOnQuit;
  $('power').classList.toggle('off', !powered); $('power').setAttribute('aria-pressed', powered);
  $('power').querySelector('span').textContent = powered ? 'Lights on' : 'Lights off';
  $('device-detail').textContent = `${settings.count} LEDs · USB HID`;
  const captureMode = ['screen', 'music'].includes(settings.mode);
  $('capture-controls').hidden = !captureMode;
  $('music-controls').hidden = settings.mode !== 'music';
  $('capture').textContent = stream ? 'Stop capture' : 'Start capture';
  $('capture-help').textContent = platform === 'darwin'
    ? 'Allow Screen & System Audio Recording in macOS Privacy settings. Microphone needs its own permission.'
    : 'Screen and audio capture stay on this computer. Start capture to begin.';
}
async function save() {
  clearTimeout(saveTimer);
  try { await window.light.settings({ ...settings }); } catch (error) { notice(`Could not save settings: ${error.message}`, true); throw error; }
}
function changed() { lastSolid = ''; updateUI(); clearTimeout(saveTimer); saveTimer = setTimeout(() => void save().catch(() => {}), 200); }
async function stopCapture() {
  captureGeneration++;
  if (stream) { const old = stream; stream = null; old.getTracks().forEach(track => { track.onended = null; track.stop(); }); }
  video.srcObject = null;
  if (audio) { await audio.close().catch(() => {}); audio = null; }
  analyser = null; audioData = null; previous = null;
  updateUI();
}
async function black() {
  while (sending) await new Promise(resolve => setTimeout(resolve, 10));
  if (connected) await window.light.solid([0, 0, 0]);
}
async function startCapture() {
  if (switching || !powered) return;
  switching = true; $('capture').disabled = true;
  try {
    await stopCapture();
    const generation = captureGeneration;
    let acquired;
    if (settings.mode === 'music' && settings.audioSource === 'microphone') {
      acquired = await navigator.mediaDevices.getUserMedia({ audio: { echoCancellation: false, noiseSuppression: false, autoGainControl: false }, video: false });
    } else {
      await window.light.prepareCapture({ screenId: settings.screenId, audio: settings.mode === 'music' });
      acquired = await navigator.mediaDevices.getDisplayMedia({ video: { frameRate: { ideal: settings.fps, max: 30 }, width: { ideal: 640 } }, audio: settings.mode === 'music' });
    }
    if (generation !== captureGeneration || !powered) { acquired.getTracks().forEach(track => track.stop()); return; }
    stream = acquired;
    if (settings.mode === 'screen') { video.srcObject = stream; await video.play(); }
    if (settings.mode === 'music') {
      if (!stream.getAudioTracks().length) throw new Error('Computer audio was not available. Select Microphone or check audio capture permissions.');
      audio = new AudioContext(); await audio.resume();
      analyser = audio.createAnalyser(); analyser.fftSize = 256; analyser.smoothingTimeConstant = 0.75;
      audio.createMediaStreamSource(stream).connect(analyser);
      audioData = new Uint8Array(analyser.frequencyBinCount);
    }
    stream.getTracks().forEach(track => { track.onended = async () => { await stopCapture(); await black(); notice('Capture ended. Start capture to resume.'); }; });
    previous = null; notice('');
  } catch (error) { await stopCapture(); await black(); notice(`Capture could not start: ${error.message}`, true); }
  finally { switching = false; $('capture').disabled = false; updateUI(); }
}
async function selectMode(mode) {
  if (switching) return;
  switching = true;
  try {
    await stopCapture(); settings.mode = mode; previous = null; lastSolid = '';
    await save(); updateUI();
    if (['screen', 'music'].includes(mode)) { await black(); notice('Click Start capture to begin.'); }
    else notice(connected ? '' : 'Close SyncLight, then connect your strip to get started.');
  } finally { switching = false; }
}
function draw(colors) {
  const w = 1000, h = 300;
  preview.clearRect(0, 0, w, h);
  const points = L.positions(settings.count, settings.sides);
  const shown = powered ? colors : colors.map(() => [28, 27, 35]);
  shown.forEach((rgb, i) => {
    const point = points[i]; if (!point) return;
    const x = 268 + point[0] * 464, y = 28 + point[1] * 224;
    preview.beginPath(); preview.shadowBlur = powered ? 28 : 0;
    preview.shadowColor = `rgb(${rgb.join(',')})`; preview.fillStyle = `rgb(${rgb.join(',')})`;
    preview.arc(x, y, 4, 0, Math.PI * 2); preview.fill();
  });
  preview.shadowBlur = 0; preview.fillStyle = '#0f1016'; preview.strokeStyle = '#49434f'; preview.lineWidth = 2;
  preview.beginPath(); preview.roundRect(281, 40, 438, 200, 7); preview.fill(); preview.stroke();
  if (settings.mode === 'screen' && stream && video.readyState >= 2) preview.drawImage(video, 289, 48, 422, 184);
  else {
    const g = preview.createLinearGradient(290, 50, 710, 235);
    const color = shown[Math.floor(shown.length / 2)] || [60, 40, 90];
    g.addColorStop(0, `rgba(${color.join(',')},0.22)`); g.addColorStop(1, '#15131d');
    preview.fillStyle = g; preview.fillRect(289, 48, 422, 184);
    preview.fillStyle = '#c9bbdf'; preview.font = '18px Segoe UI'; preview.textAlign = 'center';
    preview.fillText(settings.mode === 'music' ? '♫' : 'DYNAMIC / LIGHT', 500, 145);
  }
  preview.fillStyle = '#3b3543'; preview.fillRect(489, 242, 22, 25);
  preview.beginPath(); preview.roundRect(451, 267, 98, 5, 3); preview.fill();
}
async function tick() {
  const started = performance.now();
  try {
    let colors;
    const time = started / 1000;
    if (settings.mode === 'solid') colors = Array.from({ length: settings.count }, () => L.hexRGB(settings.color));
    else if (settings.mode === 'rainbow') colors = Array.from({ length: settings.count }, (_, i) => L.hsv((i / settings.count + time / 25) % 1, 0.8));
    else if (settings.mode === 'screen' && stream && video.readyState >= 2) {
      canvas.width = 160; canvas.height = Math.max(1, Math.round(160 * video.videoHeight / video.videoWidth));
      ctx.drawImage(video, 0, 0, canvas.width, canvas.height);
      colors = L.sampleEdges(ctx.getImageData(0, 0, canvas.width, canvas.height).data, canvas.width, canvas.height, settings.count, settings.sides);
    } else if (settings.mode === 'music' && analyser) {
      analyser.getByteFrequencyData(audioData);
      const base = L.hexRGB(settings.color);
      colors = Array.from({ length: settings.count }, (_, i) => {
        const bin = Math.min(audioData.length - 1, 2 + Math.floor(i / settings.count * 70));
        const level = Math.min(1, audioData[bin] / 255 * (0.5 + settings.sensitivity / 25));
        return base.map(c => Math.round(c * level));
      });
    } else colors = Array.from({ length: settings.count }, () => [0, 0, 0]);
    colors = L.transform(colors, settings.mode === 'solid' ? null : previous, settings);
    previous = colors; lastColors = colors; draw(colors);
    if (connected && powered && !switching) {
      sending = true;
      let written = false;
      if (settings.mode === 'solid') {
        const signature = JSON.stringify([colors[0], settings.count]);
        if (signature !== lastSolid) { written = await window.light.solid(colors[0]); if (written) lastSolid = signature; }
      } else if (settings.mode === 'rainbow' || stream) {
        written = await window.light.frame(L.reorder(colors, settings.reverse, settings.offset));
      }
      if (written) frameCounter++;
    }
    if (started - frameTime > 1000) {
      $('frame-rate').textContent = connected ? (settings.mode === 'solid' ? 'USB · Connected' : `${Math.round(frameCounter * 1000 / (started - frameTime))} FPS · USB`) : 'Preview only · Disconnected';
      frameCounter = 0; frameTime = started;
    }
  } catch (error) { notice(error.message, true); }
  finally { sending = false; timer = setTimeout(tick, Math.max(1, 1000 / settings.fps - (performance.now() - started))); }
}
async function init() {
  const initial = await window.light.init(); settings = initial.settings; platform = initial.platform;
  // Capture always requires a new explicit user action after launch.
  updateUI(); status(initial.status);
  window.light.onStatus(status);
  window.light.onPause(async options => { powered = false; await stopCapture(); if (options?.turnOff) await black(); updateUI(); });
  window.light.onResume(() => notice('Computer awake. Turn lights on; restart capture if needed.'));
  $('connect').onclick = async () => {
    $('connect').disabled = true;
    try {
      if (connected) { await black(); await window.light.disconnect(); }
      else { await save(); status(await window.light.connect()); }
    } catch (error) { notice(error.message, true); }
    finally { $('connect').disabled = false; }
  };
  $('power').onclick = async () => {
    powered = !powered; lastSolid = ''; previous = null; updateUI();
    if (!powered) { await stopCapture(); await black(); }
    else if (['screen', 'music'].includes(settings.mode)) notice('Click Start capture to resume.');
  };
  document.querySelectorAll('[data-mode]').forEach(button => { button.onclick = () => selectMode(button.dataset.mode).catch(error => notice(error.message, true)); });
  document.querySelectorAll('[data-color]').forEach(button => { button.onclick = () => { settings.color = button.dataset.color; changed(); }; });
  $('color').oninput = event => { settings.color = event.target.value; changed(); };
  for (const name of ['brightness', 'sensitivity', 'smoothing']) $(name).oninput = event => { settings[name] = Number(event.target.value); changed(); };
  for (const name of ['count', 'sides', 'offset', 'fps']) $(name).onchange = async event => {
    if (!event.target.checkValidity()) return;
    switching = true;
    try { while (sending) await new Promise(resolve => setTimeout(resolve, 10)); settings[name] = Number(event.target.value); previous = null; await save(); changed(); }
    finally { switching = false; }
  };
  for (const [id, key] of [['reverse', 'reverse'], ['off-on-sleep', 'offOnSleep'], ['off-on-quit', 'offOnQuit']]) $(id).onchange = event => { settings[key] = event.target.checked; changed(); };
  $('audio-source').onchange = async event => { settings.audioSource = event.target.value; await stopCapture(); await black(); changed(); notice('Click Start capture to use the selected audio source.'); };
  $('screen').onchange = async event => { settings.screenId = event.target.value; await stopCapture(); await black(); changed(); notice('Click Start capture to use the selected display.'); };
  $('refresh-screens').onclick = async () => {
    try {
      const screens = await window.light.screens(); $('screen').replaceChildren();
      for (const screen of screens) { const option = document.createElement('option'); option.value = screen.id; option.textContent = screen.name; $('screen').append(option); }
      if (screens.some(screen => screen.id === settings.screenId)) $('screen').value = settings.screenId;
      else { settings.screenId = screens[0]?.id || ''; changed(); }
    } catch (error) { notice(error.message, true); }
  };
  $('capture').onclick = async () => { if (stream) { await stopCapture(); await black(); } else await startCapture(); };
  tick();
}
init().catch(error => notice(`Unable to initialize: ${error.message}`, true));
