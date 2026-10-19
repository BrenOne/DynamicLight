"use strict";
const $ = (id) => document.getElementById(id);
const { invoke } = window.__TAURI__.core;
let settings,
  runtime,
  saveTimer,
  busy = false,
  queue = Promise.resolve(),
  platform = "",
  loginEnabled = false,
  loginAvailable = true;
let previewPending,
  revision = 0,
  lastError = "";
// Only unacknowledged edits are overlaid on native snapshots.
const pendingSettings = {};
function markSettings(keys) {
  for (const key of keys) pendingSettings[key] = structuredClone(settings[key]);
}
function acceptSettings(value) {
  settings = { ...structuredClone(value.settings), ...structuredClone(pendingSettings) };
}
const labels = {
  solid: "Solid color",
  screen: "Screen sync",
  blend: "Screen + music",
  music: "Music",
  rainbow: "Effects",
};
const effectLabels = {
  rainbow: "Color flow",
  aurora: "Aurora",
  breath: "Breathe",
  comet: "Comet",
  ember: "Ember",
};
const ranges = {
  brightness: "brightness",
  sensitivity: "sensitivity",
  smoothing: "smoothing",
  speed: "speed",
  "noise-floor": "noiseFloor",
  release: "release",
  "input-gain": "inputGain", blend: "blend", saturation: "saturation", gamma: "gamma",
  "max-brightness": "maxBrightness", "transition-limit": "transitionLimit", "minimum-glow": "minimumGlow",
};
const behaviorSettings = [
  ["restore-on-start", "restoreOnStart"],
  ["resume-capture", "resumeCapture"],
  ["off-on-display-sleep", "offOnDisplaySleep"],
  ["off-on-lock", "offOnLock"],
];
function notice(message, error = false) {
  $("notice-text").textContent = message;
  $("notice").hidden = !message;
  $("notice").classList.toggle("error", error);
}
$("open-compact").onclick = () => run(async () => { await save(); await invoke("compact_window", { operation: "open", pinned: null }); });
function request(action) {
  const pending = queue
    .catch(() => {})
    .then(() => invoke("control", { action }));
  queue = pending;
  return pending.then((value) => {
    renderRuntime(value);
    return value;
  });
}
function renderRuntime(value) {
  const endedCalibration = runtime?.calibrating && !value.calibrating;
  runtime = value;
  if (endedCalibration && $("calibration-dialog").open) { $("calibration-dialog").close(); settings = value.settings; queueMicrotask(updateUI); }
  if (!settings) return;
  const connected = value.status.connected;
  const current = value.settings;
  const waiting =
    ["screen", "music", "blend"].includes(current.mode) && !value.capturing;
  const active =
    connected && value.powered && !waiting && current.brightness > 0;
  $("status-dot").classList.toggle("connected", connected);
  $("live-dot").classList.toggle("active", active);
  $("status-label").textContent = connected ? "Connected" : "Not connected";
  $("connect").textContent = connected ? "Disconnect" : "Reconnect";
  $("connection-message").textContent = value.status.message;
  const running = value.requested;
  $("power").classList.toggle("running", running);
  $("power").setAttribute("aria-pressed", running);
  $("power").querySelector("span").textContent = busy
    ? "Working…"
    : running
      ? "Stop"
      : `Start ${labels[settings.mode].toLowerCase()}`;
  $("power").setAttribute(
    "aria-label",
    running ? "Stop lighting" : `Start ${labels[settings.mode].toLowerCase()}`,
  );
  $("frame-rate").textContent = !connected
    ? "Disconnected · Preview only"
    : !value.powered
      ? "Lights off"
      : waiting
        ? "Capture stopped · Strip dark"
        : current.brightness === 0
          ? "Brightness zero · Strip dark"
          : value.capturing || current.mode === "rainbow"
            ? `${value.fps} FPS · USB`
            : "Solid color · USB";
  $("activity-label").textContent = !connected
    ? "Offline"
    : active
      ? "Sending"
      : "Standby";
  $("usb-fps").textContent = connected ? value.fps : "—";
  $("write-time").textContent =
    connected && value.lastWriteMs !== null
      ? value.writeDurationMs.toFixed(1)
      : "—";
  $("last-write").textContent =
    value.lastWriteMs === null
      ? "Never"
      : value.lastWriteMs < 1000
        ? `${value.lastWriteMs} ms ago`
        : `${Math.floor(value.lastWriteMs / 1000)} s ago`;
  $("frames-written").textContent = value.framesWritten.toLocaleString();
  $("dropped-frames").textContent = (value.droppedFrames || 0).toLocaleString();
  $("write-errors").textContent = value.writeErrors.toLocaleString();
  $("capture-size").textContent = value.captureSize[0]
    ? value.captureSize.join(" × ")
    : "—";
  $("audio-level").textContent =
    ["music", "blend"].includes(current.mode) && value.capturing
      ? `${Math.round(value.audioLevel * 100)}% RMS`
      : "—";
  $("audio-status").textContent = !value.capturing
    ? "Capture stopped"
    : !value.audioSamples
      ? "Waiting for audio"
      : value.audioLevel <= current.noiseFloor / 100
        ? "Quiet / below gate"
        : "Audio arriving";
  const message = value.pauseReason
    ? `${value.pauseReason}${value.requested ? " · lighting paused" : ""}`
    : value.captureRequiresStart
      ? (value.error ? `${value.error} ` : "") + "Capture needs a restart. Click Retry to continue."
      : value.error || (value.recovering ? "Reconnecting… Stop cancels recovery." : "");
  if (message !== lastError || (message && $("notice").hidden)) notice(message, !!value.error && !value.pauseReason);
  lastError = message;
  $("retry").hidden = !value.requested || !!value.pauseReason || !(value.error || value.captureRequiresStart || value.recovering);
  $("retry").disabled = busy;
  $("lifecycle-warning").textContent = value.lifecycleWarning || "";
  $("lifecycle-warning").hidden = !value.lifecycleWarning;
  if (value.pauseReason) $("frame-rate").textContent = value.pauseReason;
  else if (value.recovering) $("frame-rate").textContent = "Waiting to reconnect";
  draw(value.colors);
  drawSpectrum(value.audioBands);
  updateSceneSelection();
}
function selectButtons(selector, key, selected) {
  document.querySelectorAll(selector).forEach((button) => {
    const match = button.dataset[key] === selected;
    button.classList.toggle("selected", match);
    button.setAttribute("aria-pressed", match);
  });
}
function updateUI() {
  selectButtons("[data-mode]", "mode", settings.mode);
  selectButtons("[data-effect]", "effect", settings.effect);
  selectButtons("[data-music]", "music", settings.musicStyle);
  selectButtons("[data-color]", "color", settings.color.toLowerCase());
  $("mode-badge").textContent =
    settings.mode === "rainbow"
      ? effectLabels[settings.effect]
      : labels[settings.mode];
  $("color").value = settings.color;
  $("hex-value").textContent = settings.color.toUpperCase();
  for (const [id, key] of Object.entries(ranges)) {
    $(id).value = settings[key];
    $(id + "-value").textContent = settings[key] + "%";
  }
  for (const name of ["count", "sides", "offset", "fps", "palette"])
    $(name).value = settings[name];
  $("input-gain-value").textContent = settings.inputGain + " dB";
  $("gamma-value").textContent = (settings.gamma / 100).toFixed(2);
  $("transition-limit-value").textContent = settings.transitionLimit ? settings.transitionLimit + "% / s" : "Off";
  $("exclude-bars").checked = !!settings.excludeBars;
  ["red", "green", "blue"].forEach((c, i) => { $(c + "-balance").value = settings.rgbBalance[i]; $(c + "-balance-value").textContent = settings.rgbBalance[i] + "%"; });
  $("screen-tuning").hidden = !["screen", "blend"].includes(settings.mode);
  $("blend-control").hidden = settings.mode !== "blend";
  document.querySelector(".music-styles").hidden = settings.mode === "blend";
  $("audio-source").value = settings.audioSource;
  $("reverse").checked = settings.reverse;
  $("effect-direction").value = settings.effectReverse ? "reverse" : "forward";
  $("effect-direction-control").hidden = settings.mode !== "rainbow";
  $("off-on-sleep").checked = settings.offOnSleep;
  $("off-on-quit").checked = settings.offOnQuit;
  for (const [id, key] of behaviorSettings) $(id).checked = !!settings[key];
  $("launch-at-login").checked = loginEnabled;
  $("launch-at-login").disabled = busy || !loginAvailable;
  $("device-detail").textContent = `${settings.count} LEDs · USB HID`;
  $("led-total").textContent = `→ ${settings.count}`;
  $("capture-controls").hidden = !["screen", "music", "blend"].includes(settings.mode);
  $("display-controls").hidden = !["screen", "blend"].includes(settings.mode);
  $("music-controls").hidden = !["music", "blend"].includes(settings.mode);
  $("effect-controls").hidden = settings.mode !== "rainbow";
  $("palette-controls").hidden = !["music", "rainbow"].includes(settings.mode);
  const fixedPalette =
    settings.mode === "rainbow" &&
    ["rainbow", "ember"].includes(settings.effect);
  $("palette").disabled = busy || fixedPalette;
  $("palette-help").textContent = fixedPalette
    ? "Fixed colors for this effect."
    : "";
  $("color-control").hidden =
    ["screen", "blend"].includes(settings.mode) ||
    (settings.mode !== "solid" &&
      (settings.palette !== "mono" || fixedPalette));
  $("speed-controls").hidden =
    settings.mode !== "rainbow" &&
    !(settings.mode === "music" && settings.musicStyle === "wave");
  $("smoothing").disabled = busy || settings.mode === "solid";
  $("screen").disabled = busy || !["screen", "blend"].includes(settings.mode);
  $("refresh-screens").disabled = busy || !["screen", "blend"].includes(settings.mode);
  $("capture-help").textContent =
    ["screen", "blend"].includes(settings.mode)
      ? "Capture stays on this computer. " +
        (platform === "linux"
          ? "Screen sync needs an X11 session; Wayland support is planned."
          : platform === "macos"
            ? "Allow Screen Recording in Privacy settings."
            : "Only edge colors are sent to your strip.")
      : "Audio is analyzed locally. " +
        (platform === "linux"
          ? "Computer audio uses PulseAudio or PipeWire."
          : platform === "macos"
            ? "Allow Screen & System Audio Recording for computer audio, or Microphone access for your mic."
            : "Computer audio follows your speakers. Microphone listens to your room.");
  if (runtime) renderRuntime(runtime);
  refreshEffectPreviews();
  window.LibraryTools?.sync();
}
// Coalesce drag updates: at most one preview IPC in flight, then the newest value.
// Disk writes happen after the user pauses, not on each animation frame.
function previewSettings() {
  if (!previewPending) {
    previewPending = (async () => {
      while (Object.keys(pendingSettings).length) {
        const patch = structuredClone(pendingSettings);
        const value = await request({ type: "patchSettings", patch, persist: false });
        for (const [key, sent] of Object.entries(patch)) {
          if (JSON.stringify(pendingSettings[key]) === JSON.stringify(sent)) delete pendingSettings[key];
        }
        acceptSettings(value);
      }
    })().catch((error) => {
      notice(String(error), true);
      $("save-state").textContent = "Not saved";
      throw error;
    }).finally(() => { previewPending = undefined; });
  }
  return previewPending;
}
async function save() {
  clearTimeout(saveTimer);
  saveTimer = null;
  await previewSettings();
  const savingRevision = revision;
  $("save-state").textContent = "Saving…";
  try {
    const result = await request({
      type: "patchSettings",
      patch: {},
      persist: true,
    });
    acceptSettings(result);
    await window.LibraryTools?.remember(result);
    if (savingRevision === revision) $("save-state").textContent = "Saved";
    return result;
  } catch (error) {
    $("save-state").textContent = "Not saved";
    throw error;
  }
}
function changed(keys) {
  markSettings(keys);
  revision++;
  updateUI();
  $("save-state").textContent = "Applying…";
  previewSettings().catch(() => {});
  clearTimeout(saveTimer);
  saveTimer = setTimeout(
    () => save().catch((error) => notice(String(error), true)),
    650,
  );
}
async function run(action) {
  if (busy) return;
  busy = true;
  document.querySelectorAll("input, select").forEach((input) => {
    input.disabled = true;
  });
  const controls = ["connect", "power", "refresh-screens", "retry"];
  controls.forEach((id) => {
    $(id).disabled = true;
  });
  document
    .querySelectorAll("[data-mode], [data-effect], [data-music]")
    .forEach((button) => {
      button.disabled = true;
    });
  $("power").querySelector("span").textContent = "Working…";
  notice("");
  try {
    await action();
  } catch (error) {
    notice(String(error), true);
  } finally {
    busy = false;
    document.querySelectorAll("input, select").forEach((input) => {
      input.disabled = false;
    });
    controls.forEach((id) => {
      $(id).disabled = false;
    });
    document
      .querySelectorAll("[data-mode], [data-effect], [data-music]")
      .forEach((button) => {
        button.disabled = false;
      });
    updateUI();
  }
}
function positions(count, sides) {
  const aspect = 16 / 9,
    perimeter = 2 + aspect * (sides === 4 ? 2 : 1);
  return Array.from({ length: count }, (_, i) => {
    const p = ((i + 0.5) / count) * perimeter;
    if (p < 1) return [0, 1 - p];
    if (p < 1 + aspect) return [(p - 1) / aspect, 0];
    if (p < 2 + aspect) return [1, p - 1 - aspect];
    return [1 - (p - 2 - aspect) / aspect, 1];
  });
}
function draw(colors) {
  const ctx = $("preview").getContext("2d");
  ctx.clearRect(0, 0, 1000, 420);
  const s = runtime.settings;
  const shown = runtime.powered ? colors : colors.map(() => [0, 0, 0]);
  const points = positions(shown.length, s.sides);
  ctx.strokeStyle = "#283a402b";
  ctx.lineWidth = 1;
  for (let x = 20; x < 1000; x += 40) {
    ctx.beginPath();
    ctx.moveTo(x, 20);
    ctx.lineTo(x, 400);
    ctx.stroke();
  }
  for (let y = 20; y < 420; y += 40) {
    ctx.beginPath();
    ctx.moveTo(20, y);
    ctx.lineTo(980, y);
    ctx.stroke();
  }
  // The glow uses actual generated colors, including silence and lights-off black.
  shown.forEach((rgb, i) => {
    const [x, y] = points[i],
      px = 235 + x * 530,
      py = 55 + y * 275;
    const glow = ctx.createRadialGradient(px, py, 0, px, py, 75);
    glow.addColorStop(0, `rgba(${rgb.join(",")},0.13)`);
    glow.addColorStop(1, `rgba(${rgb.join(",")},0)`);
    ctx.fillStyle = glow;
    ctx.fillRect(px - 75, py - 75, 150, 150);
  });
  ctx.fillStyle = "#0c1419";
  ctx.strokeStyle = "#4b6269";
  ctx.lineWidth = 2;
  ctx.beginPath();
  ctx.roundRect(247, 66, 506, 253, 11);
  ctx.fill();
  ctx.stroke();
  const color = shown[Math.floor(shown.length / 2)] || [0, 0, 0];
  const gradient = ctx.createLinearGradient(260, 80, 730, 310);
  gradient.addColorStop(0, `rgba(${color.join(",")},0.15)`);
  gradient.addColorStop(1, "#10191e");
  ctx.fillStyle = gradient;
  ctx.fillRect(257, 76, 486, 232);
  ctx.fillStyle = "#d5e7e0";
  ctx.font = "500 24px Segoe UI";
  ctx.textAlign = "center";
  ctx.fillText(
    s.mode === "rainbow" ? effectLabels[s.effect] : labels[s.mode],
    500,
    184,
  );
  ctx.font = "14px Segoe UI";
  ctx.fillStyle = "#94a8b2";
  ctx.fillText(
    isRunning(runtime)
      ? s.brightness === 0
        ? "Brightness is 0%"
        : "Running"
      : runtime.pauseReason || (runtime.recovering ? "Reconnecting…" : runtime.captureRequiresStart ? "Capture paused · click Retry" : "Stopped · select a mode to start"),
    500,
    216,
  );
  shown.forEach((rgb, i) => {
    const [x, y] = points[i];
    ctx.beginPath();
    ctx.fillStyle = rgb.some((c) => c > 0)
      ? `rgb(${rgb.join(",")})`
      : "#34434a";
    ctx.arc(235 + x * 530, 55 + y * 275, 3.2, 0, Math.PI * 2);
    ctx.fill();
  });
  ctx.fillStyle = "#283a40";
  ctx.fillRect(486, 321, 28, 29);
  ctx.beginPath();
  ctx.roundRect(439, 350, 122, 5, 3);
  ctx.fill();
  ctx.fillStyle = "#6e868f";
  ctx.font = "10px Segoe UI";
  ctx.fillText(`${s.count} LEDS  /  ${s.sides}-SIDE LAYOUT`, 500, 384);
  const strip = $("strip-preview").getContext("2d");
  strip.clearRect(0, 0, 1000, 18);
  shown.forEach((_, i) => {
    const physicalTest = runtime.calibrating && runtime.calibrationLed !== null;
    const j = (i + (physicalTest ? 0 : s.offset)) % shown.length,
      rgb = shown[s.reverse && !physicalTest ? shown.length - 1 - j : j];
    strip.fillStyle = `rgb(${rgb.join(",")})`;
    strip.fillRect(
      (i * 1000) / shown.length,
      0,
      Math.max(1, 1000 / shown.length - 1),
      18,
    );
  });
}
function drawSpectrum(bands) {
  if ($("music-controls").hidden) return;
  const ctx = $("spectrum").getContext("2d");
  ctx.clearRect(0, 0, 800, 110);
  bands.forEach((v, i) => {
    const h = Math.max(2, v * 100);
    ctx.fillStyle = `hsl(${155 + i * 2.7} 65% ${50 + v * 16}%)`;
    ctx.globalAlpha = 0.25 + v * 0.75;
    ctx.beginPath();
    ctx.roundRect(i * 25 + 3, 108 - h, 15, h, 3);
    ctx.fill();
  });
  ctx.globalAlpha = 1;
}

function isRunning(value) {
  return (
    value?.status.connected &&
    value.powered &&
    (!["screen", "music", "blend"].includes(value.settings.mode) || value.capturing)
  );
}
async function startLighting() {
  await save();
  await request({ type: "start" });
}
function changeSource(key, value) {
  const resume = runtime.requested;
  return run(async () => {
    settings[key] = value;
    markSettings([key]);
    if (resume) await startLighting();
    else await save();
  });
}
async function refreshDisplays() {
  const screens = await invoke("displays");
  $("screen").replaceChildren();
  const primary = document.createElement("option");
  primary.value = "";
  primary.textContent = "Primary display";
  $("screen").append(primary);
  for (const screen of screens) {
    const option = document.createElement("option");
    option.value = screen.id;
    option.textContent = screen.name;
    $("screen").append(option);
  }
  if (
    settings.screenId &&
    !screens.some((screen) => screen.id === settings.screenId)
  ) {
    const missing = document.createElement("option");
    missing.value = settings.screenId;
    missing.textContent = "Saved display (unavailable)";
    $("screen").append(missing);
  }
  $("screen").value = settings.screenId;
}
const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)");
let thumbnailTimer,
  thumbnailBusy = false,
  thumbnailsVisible = true,
  thumbnailsFocused = true;
async function refreshEffectPreviews() {
  clearTimeout(thumbnailTimer);
  if (
    !settings ||
    thumbnailBusy ||
    $("effect-controls").hidden ||
    !thumbnailsVisible ||
    !thumbnailsFocused ||
    document.hidden
  )
    return;
  thumbnailBusy = true;
  const key = JSON.stringify(settings);
  try {
    const frames = await invoke("effect_previews", {
      settings: { ...settings },
      seconds: reducedMotion.matches ? 2 : (performance.now() / 1000) % 86400,
    });
    if (!$("effect-controls").hidden)
      document
        .querySelectorAll(".effect-art")
        .forEach((canvas, i) => drawEffectThumbnail(canvas, frames[i]));
  } catch (error) {
    notice("Effect preview unavailable: " + String(error), true);
  } finally {
    thumbnailBusy = false;
    if (!reducedMotion.matches || key !== JSON.stringify(settings))
      thumbnailTimer = setTimeout(refreshEffectPreviews, 100);
  }
}
function drawEffectThumbnail(canvas, colors) {
  const ctx = canvas.getContext("2d");
  ctx.clearRect(0, 0, 240, 150);
  const points = positions(colors.length, 3);
  // Lit segments and their spill follow the actual native effect samples.
  ctx.fillStyle = "#101b21";
  ctx.strokeStyle = "#2b3b43";
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.roundRect(43, 37, 154, 76, 5);
  ctx.fill();
  ctx.stroke();
  colors.forEach((rgb, i) => {
    const [x, y] = points[i],
      px = 34 + x * 172,
      py = 28 + y * 94;
    const glow = ctx.createRadialGradient(px, py, 0, px, py, 24);
    glow.addColorStop(0, `rgba(${rgb.join(",")},.22)`);
    glow.addColorStop(1, `rgba(${rgb.join(",")},0)`);
    ctx.fillStyle = glow;
    ctx.fillRect(px - 24, py - 24, 48, 48);
  });
  colors.forEach((rgb, i) => {
    const [x, y] = points[i];
    ctx.fillStyle = rgb.some((c) => c > 1)
      ? `rgb(${rgb.join(",")})`
      : "#29363c";
    ctx.beginPath();
    ctx.roundRect(32 + x * 172, 26 + y * 94, 4, 4, 1.2);
    ctx.fill();
  });
  ctx.fillStyle = "#2b3b43";
  ctx.fillRect(117, 114, 6, 9);
  ctx.fillRect(104, 123, 32, 2);
}

async function init() {
  platform = await invoke("platform_name");
  if (platform === "linux")
    $("close-behavior").textContent =
      "Closing the window minimizes it and keeps lighting running. Use Quit to exit.";
  await window.__TAURI__.event.listen("lighting", (event) =>
    renderRuntime(event.payload),
  );
  const initial = await request({ type: "snapshot" });
  settings = initial.settings;
  try { loginEnabled = await invoke("launch_at_login", { enabled: null }); }
  catch (error) { loginAvailable = false; notice(`Launch at login is unavailable: ${error}`, true); }
  await initProfiles();
  window.LibraryTools.init();
  initCalibration();
  $("exclude-bars").onchange = e => { settings.excludeBars = e.target.checked; changed(["excludeBars"]); };
  ["red", "green", "blue"].forEach((c, i) => { $(c + "-balance").oninput = e => { settings.rgbBalance[i] = Number(e.target.value); changed(["rgbBalance"]); }; });
  $("reset-screen").onclick = () => { Object.assign(settings, { excludeBars: false, rgbBalance: [100,100,100], saturation: 100, gamma: 100 }); changed(["excludeBars", "rgbBalance", "saturation", "gamma"]); };
  updateUI();
  $("connect").onclick = () =>
    run(async () => {
      await save();
      await request({
        type: runtime.status.connected ? "disconnect" : "connect",
      });
    });
  $("power").onclick = () =>
    run(async () => {
      if (runtime.requested) {
        await request({ type: "power", on: false });
        await save();
      } else await startLighting();
    });
  $("retry").onclick = () => run(startLighting);
  $("launch-at-login").onchange = (event) => {
    const enabled = event.target.checked;
    run(async () => {
      loginEnabled = await invoke("launch_at_login", { enabled });
      if (loginEnabled !== enabled) throw new Error("Login startup was not changed. Check your operating system's login-item settings.");
    });
  };
  $("effect-direction").onchange = () => { settings.effectReverse = $("effect-direction").value === "reverse"; changed(["effectReverse"]); };
  document.querySelectorAll("[data-mode]").forEach((button) => {
    button.setAttribute(
      "aria-label",
      `Start ${labels[button.dataset.mode].toLowerCase()}`,
    );
    button.onclick = () =>
      run(async () => {
        await save();
        settings = (await request({ type: "selectMode", mode: button.dataset.mode })).settings;
        updateUI();
        await startLighting();
        if (["screen", "blend"].includes(settings.mode)) await refreshDisplays();
      });
  });
  for (const [selector, dataset, key] of [
    ["[data-effect]", "effect", "effect"],
    ["[data-music]", "music", "musicStyle"],
  ])
    document.querySelectorAll(selector).forEach((button) => {
      button.onclick = () =>
        run(async () => {
          settings[key] = button.dataset[dataset];
          markSettings([key]);
          updateUI();
          await startLighting();
        });
    });
  document.querySelectorAll("[data-color]").forEach((button) => {
    button.onclick = () => {
      settings.color = button.dataset.color;
      changed(["color"]);
    };
  });
  $("color").oninput = (event) => {
    settings.color = event.target.value;
    changed(["color"]);
  };
  for (const [id, key] of Object.entries(ranges))
    $(id).oninput = (event) => {
      settings[key] = Number(event.target.value);
      changed([key]);
    };
  for (const name of ["count", "sides", "offset", "fps"])
    $(name).onchange = (event) => {
      if (event.target.checkValidity()) {
        settings[name] = Number(event.target.value);
        changed([name]);
      }
    };
  $("palette").onchange = (event) => {
    settings.palette = event.target.value;
    changed(["palette"]);
  };
  for (const [id, key] of [
    ["reverse", "reverse"],
    ["off-on-sleep", "offOnSleep"],
    ["off-on-quit", "offOnQuit"],
    ...behaviorSettings,
  ])
    $(id).onchange = (event) => {
      settings[key] = event.target.checked;
      changed([key]);
    };
  $("audio-source").onchange = (event) =>
    changeSource("audioSource", event.target.value);
  $("screen").onchange = (event) =>
    changeSource("screenId", event.target.value);
  $("refresh-screens").onclick = () => run(refreshDisplays);
  $("quit").onclick = () =>
    run(async () => {
      await save();
      await invoke("quit");
    });
  const observer = new IntersectionObserver((entries) => {
    thumbnailsVisible = entries[0].isIntersecting;
    refreshEffectPreviews();
  });
  observer.observe($("effect-controls"));
  window.addEventListener("blur", () => {
    thumbnailsFocused = false;
    clearTimeout(thumbnailTimer);
  });
  window.addEventListener("focus", () => {
    thumbnailsFocused = true;
    refreshEffectPreviews();
  });
  document.addEventListener("visibilitychange", refreshEffectPreviews);
  reducedMotion.addEventListener("change", refreshEffectPreviews);
  if (["screen", "blend"].includes(settings.mode))
    refreshDisplays().catch((error) => notice(String(error), true));
  window.addEventListener("focus", () =>
    request({ type: "snapshot" }).then(value => { if (!busy && !saveTimer && !previewPending) { settings = value.settings; updateUI(); } }).catch((error) => notice(String(error), true)),
  );
}
function initCalibration() {
  const dialog = $("calibration-dialog");
  let draft, edgePreview = false;
  async function step() {
    if (!runtime?.calibrating) return false;
    for (const id of ["cal-count", "cal-led", "cal-offset"]) if (!$(id).reportValidity()) return false;
    draft.count = Number($("cal-count").value);
    draft.sides = Number($("cal-sides").value);
    draft.offset = Number($("cal-offset").value);
    draft.reverse = $("cal-reverse").checked;
    const led = Math.min(draft.count, Number($("cal-led").value));
    $("cal-led").value = led;
    await request({ type: "calibrationStep", settings: draft, led: edgePreview ? null : led - 1 });
    $("cal-status").textContent = edgePreview ? "Mapped edges: red left · green top · blue right · amber bottom" : `Physical LED ${led} of ${draft.count}`;
    return true;
  }
  const safe = action => async () => { try { await action(); } catch (e) { $("cal-status").textContent = String(e); } };
  $("calibrate").onclick = () => run(async () => {
    await save();
    draft = structuredClone(settings); edgePreview = false;
    await request({ type: "calibrationStart" });
    $("cal-count").value = draft.count; $("cal-led").value = 1;
    $("cal-sides").value = draft.sides; $("cal-offset").value = draft.offset; $("cal-reverse").checked = draft.reverse;
    dialog.showModal(); await step();
  });
  for (const id of ["cal-count", "cal-sides", "cal-offset", "cal-reverse"]) $(id).onchange = safe(step);
  $("cal-led").onchange = safe(async () => { edgePreview = false; await step(); });
  $("cal-prev").onclick = safe(async () => { edgePreview = false; $("cal-led").value = Math.max(1, Number($("cal-led").value) - 1); await step(); });
  $("cal-next").onclick = safe(async () => { edgePreview = false; $("cal-led").value = Math.min(draft.count, Number($("cal-led").value) + 1); await step(); });
  $("cal-preview").onclick = safe(async () => { edgePreview = true; await step(); });
  async function end(apply, stop = false) {
    const state = await request(stop ? { type: "power", on: false } : { type: "calibrationEnd", apply });
    settings = state.settings; dialog.close(); updateUI();
  }
  $("cal-apply").onclick = safe(async () => { if (await step()) await end(true); });
  $("cal-cancel").onclick = safe(() => end(false));
  $("cal-stop").onclick = safe(() => end(false, true));
  dialog.addEventListener("cancel", e => { e.preventDefault(); safe(() => end(false))(); });
}
let library = { profiles: [] }, starters = { profiles: [] };
async function initProfiles() {
  library = await invoke("profile_library", { library: null });
  starters = await invoke("starter_scenes");
  renderProfiles();
  $("scene-search").oninput = renderProfiles;
  $("scene-filter").onchange = renderProfiles;
  document.querySelector(".profiles").addEventListener("toggle", refreshScenePreviews);
  await window.__TAURI__.event.listen("external-settings", async () => {
    try {
      // Finish local field edits, then read the authoritative worker state.
      // Do not replay a full draft over a tray/profile/Quick-controls change.
      await previewPending?.catch(() => {});
      const latest = await request({ type: "snapshot" });
      acceptSettings(latest); renderRuntime(latest); updateUI();
      if (["screen", "blend"].includes(settings.mode)) await refreshDisplays();
    } catch (error) { notice(String(error), true); }
  });
  await window.__TAURI__.event.listen("control-error", ({ payload }) => notice(payload, true));
  $("profile-save").onclick = () => run(async () => {
    await save();
    const name = $("profile-name").value.trim();
    if (!name) throw new Error("Enter a profile name.");
    await editLibrary(next => next.profiles.push({ id: crypto.randomUUID(), name, favorite: true, shortcut: "", settings: { ...settings, modeSettings: {} } }));
    $("profile-name").value = "";
  });
}
async function editLibrary(edit) {
  const next = structuredClone(library); edit(next);
  try {
    library = await invoke("profile_library", { library: next });
    $("profile-message").textContent = "Saved"; renderProfiles();
  } catch (e) { $("profile-message").textContent = String(e); throw e; }
}
function renderProfiles() {
  window.LibraryTools?.sync();
  lastScenePreviewKey = "";
  $("profile-list").replaceChildren();
  library.profiles.forEach((p, i) => {
    if (!sceneVisible(p, false)) return;
    const row = document.createElement("div"); row.className = "profile-row scene-card"; row.setAttribute("role", "group"); row.setAttribute("aria-label", p.name);
    const apply = document.createElement("button"); apply.className = "profile-apply";
    decorateScene(apply, p);
    apply.onclick = () => run(async () => { await save(); const state = await invoke("apply_profile", { id: p.id }); settings = state.settings; renderRuntime(state); });
    row.append(apply);
    const favorite = document.createElement("button"); favorite.className = "scene-favorite text-button"; favorite.textContent = p.favorite ? "★ Favorite" : "☆ Favorite"; favorite.setAttribute("aria-pressed", String(p.favorite)); favorite.onclick = () => run(() => editLibrary(n => { n.profiles[i].favorite = !p.favorite; })); row.append(favorite);
    const editor = document.createElement("details"); const summary = document.createElement("summary"); summary.textContent = "Edit"; editor.append(summary); row.append(editor);
    const fields = document.createElement("div"); fields.className = "profile-fields"; editor.append(fields);
    const field = (label, value) => {
      const wrap = document.createElement("label"); wrap.textContent = label;
      const input = document.createElement("input"); input.value = value; input.maxLength = 60;
      wrap.append(input); fields.append(wrap); return input;
    };
    const name = field("Name", p.name); name.maxLength = 50;
    const shortcut = field("Global shortcut (optional)", p.shortcut); shortcut.placeholder = "Control+Shift+1";
    const button = (text, action) => {
      const b = document.createElement("button"); b.className = "secondary"; b.textContent = text;
      b.onclick = () => run(action); fields.append(b); return b;
    };
    button("Save name & shortcut", () => editLibrary(n => { n.profiles[i].name = name.value.trim(); n.profiles[i].shortcut = shortcut.value.trim(); }));
    button(p.favorite ? "★ Favorite" : "☆ Favorite", () => editLibrary(n => { n.profiles[i].favorite = !p.favorite; })).setAttribute("aria-pressed", String(p.favorite));
    button("Update settings", async () => { await save(); await editLibrary(n => { n.profiles[i].settings = { ...settings, modeSettings: {} }; n.profiles[i].source = { screenId: settings.screenId }; }); });
    button("Duplicate", () => editLibrary(n => { n.profiles.splice(i + 1, 0, { ...structuredClone(p), id: crypto.randomUUID(), name: (p.name + " copy").slice(0, 50), shortcut: "" }); }));
    button("Move up", () => editLibrary(n => { [n.profiles[i-1], n.profiles[i]] = [n.profiles[i], n.profiles[i-1]]; })).disabled = i === 0;
    button("Move down", () => editLibrary(n => { [n.profiles[i+1], n.profiles[i]] = [n.profiles[i], n.profiles[i+1]]; })).disabled = i === library.profiles.length - 1;
    button("Delete", () => editLibrary(n => { n.profiles.splice(i, 1); }));
    $("profile-list").append(row);
  });
  if (!library.profiles.length) $("profile-message").textContent = "Save a scene to add it to favorites or assign a shortcut.";
  $("starter-list").replaceChildren();
  for (const p of starters.profiles) {
    if (!sceneVisible(p, true)) continue;
    const row = document.createElement("div"); row.className = "scene-card starter-card"; row.setAttribute("role", "group"); row.setAttribute("aria-label", p.name);
    const apply = document.createElement("button"); apply.className = "profile-apply"; decorateScene(apply,p);
    apply.onclick = () => run(async () => { await save(); const state = await invoke("apply_starter", { id:p.id }); settings = state.settings; renderRuntime(state); });
    const copy = document.createElement("button"); copy.className = "text-button"; copy.textContent = "Save a copy";
    copy.onclick = () => run(() => editLibrary(n => n.profiles.push({ ...structuredClone(p), id:crypto.randomUUID(), favorite:true })));
    row.append(apply,copy); $("starter-list").append(row);
  }
  $("scene-empty").hidden = !!document.querySelector(".scene-card");
  refreshScenePreviews();
}
init().catch((error) => notice(`Unable to initialize: ${error}`, true));

async function refreshDiagnostics(enabled = null, clear = false) {
  const report = await invoke("diagnostic_report", { enabled, clear });
  $("diagnostic-text").value = report.text;
  $("diagnostic-enabled").checked = report.enabled;
}
async function diagnosticAction(action) {
  const controls = $("diagnostic-dialog").querySelectorAll("button, input");
  controls.forEach(c => c.disabled = true);
  $("diagnostic-status").textContent = "";
  try { await action(); } catch (e) { $("diagnostic-status").textContent = String(e); }
  finally { controls.forEach(c => c.disabled = false); }
}
$("diagnostic-preview").onclick = () => { $("diagnostic-dialog").showModal(); diagnosticAction(() => refreshDiagnostics()); };
$("diagnostic-close").onclick = () => $("diagnostic-dialog").close();
$("diagnostic-enabled").onchange = () => diagnosticAction(async () => {
  try { await refreshDiagnostics($("diagnostic-enabled").checked); }
  catch (e) { $("diagnostic-enabled").checked = !$("diagnostic-enabled").checked; throw e; }
});
$("diagnostic-clear").onclick = () => diagnosticAction(async () => { await refreshDiagnostics(null, true); $("diagnostic-status").textContent = "History cleared."; });
$("diagnostic-copy").onclick = () => diagnosticAction(async () => {
  try { await navigator.clipboard.writeText($("diagnostic-text").value); $("diagnostic-status").textContent = "Copied."; }
  catch { $("diagnostic-text").focus(); $("diagnostic-text").select(); $("diagnostic-status").textContent = "Select Copy from the text menu, or press Ctrl+C / ⌘C."; }
});
$("diagnostic-export").onclick = () => diagnosticAction(async () => {
  const path = await invoke("export_diagnostics"); $("diagnostic-status").textContent = "Saved to " + path;
});

let sceneTimer, scenePreviewBusy = false, scenePreviewDirty = false, lastScenePreviewKey = "";
function sceneVisible(p, starter) {
  const filter = $("scene-filter").value;
  const query = $("scene-search").value.trim().toLowerCase();
  return !(filter === "favorites" && !p.favorite || filter === "saved" && starter || filter === "starters" && !starter) && (p.name + " " + labels[p.settings.mode]).toLowerCase().includes(query);
}
function decorateScene(button,p) {
  button.scene = p;
  const canvas = document.createElement("canvas"); canvas.className = "scene-art"; canvas.width=240; canvas.height=150; canvas.setAttribute("aria-hidden","true");
  const title = document.createElement("strong"); title.textContent = p.name;
  const meta = document.createElement("small"); const s=p.settings;
  meta.textContent = (s.mode === "rainbow" ? effectLabels[s.effect] : labels[s.mode]) + " · " + s.brightness + "%" + (s.mode === "rainbow" ? " · " + s.speed + "% speed" : "");
  const status = document.createElement("span"); status.className="scene-tag"; status.textContent="Example preview";
  button.append(canvas,title,meta,status); button.setAttribute("aria-label", "Start " + p.name);
}
async function refreshScenePreviews() {
  clearTimeout(sceneTimer);
  if (scenePreviewBusy) { scenePreviewDirty = true; return; }
  if (!document.querySelector(".profiles").open || document.hidden || !thumbnailsFocused) return;
  scenePreviewDirty = false;
  scenePreviewBusy=true;
  try {
    const buttons=[...document.querySelectorAll(".scene-card .profile-apply")].filter(b => { const r=b.getBoundingClientRect(); return r.bottom>0 && r.top<innerHeight; });
    const key = JSON.stringify(buttons.map(b => [b.scene.id, b.scene.settings]));
    if (reducedMotion.matches && key === lastScenePreviewKey) return;
    for (let i=0;i<buttons.length;i+=8) {
      const chunk=buttons.slice(i,i+8);
      const frames=await invoke("scene_previews",{settings:chunk.map(b=>b.scene.settings),seconds:reducedMotion.matches?2:(performance.now()/1000)%86400});
      chunk.forEach((b,j)=>{ if(b.isConnected) drawEffectThumbnail(b.querySelector("canvas"),frames[j]); });
    }
    lastScenePreviewKey = reducedMotion.matches ? key : "";
    updateSceneSelection();
  } catch(e) { $("profile-message").textContent="Scene previews unavailable: "+String(e); }
  finally { scenePreviewBusy=false; if(scenePreviewDirty)lastScenePreviewKey=""; if(scenePreviewDirty || !reducedMotion.matches) sceneTimer=setTimeout(refreshScenePreviews,150); }
}
function updateSceneSelection() {
  if(!runtime) return;
  const common=["mode","brightness","smoothing"];
  const keys={solid:["color"],rainbow:["effect","palette","customPalette","color","speed","effectReverse"],music:["musicStyle","palette","customPalette","color","audioSource","inputGain","sensitivity","noiseFloor","release","speed"],screen:["screenId","excludeBars","rgbBalance","saturation","gamma"],blend:["screenId","audioSource","blend","inputGain","sensitivity","noiseFloor","release","excludeBars","rgbBalance","saturation","gamma"]};
  for(const b of document.querySelectorAll(".scene-card .profile-apply")) {
    const s={...b.scene.settings, screenId:b.scene.source?.screenId ?? b.scene.settings.screenId ?? ""};
    const match=!runtime.calibrating && [...common,...keys[s.mode]].every(k=>JSON.stringify(s[k])===JSON.stringify(runtime.settings[k]));
    b.classList.toggle("scene-current",match);
    b.querySelector(".scene-tag").textContent = match ? "Current settings · example" : "Example preview";
  }
}
document.addEventListener("scroll",()=>{if(reducedMotion.matches)refreshScenePreviews();},{passive:true});
window.addEventListener("focus",refreshScenePreviews);
document.addEventListener("visibilitychange",refreshScenePreviews);
reducedMotion.addEventListener("change",refreshScenePreviews);
