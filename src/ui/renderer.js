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
  previewDirty = false,
  revision = 0,
  lastError = "";
const labels = {
  solid: "Solid color",
  screen: "Screen sync",
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
  runtime = value;
  if (!settings) return;
  const connected = value.status.connected;
  const current = value.settings;
  const waiting =
    ["screen", "music"].includes(current.mode) && !value.capturing;
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
  $("write-errors").textContent = value.writeErrors.toLocaleString();
  $("capture-size").textContent = value.captureSize[0]
    ? value.captureSize.join(" × ")
    : "—";
  $("audio-level").textContent =
    current.mode === "music" && value.capturing
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
  $("audio-source").value = settings.audioSource;
  $("reverse").checked = settings.reverse;
  $("off-on-sleep").checked = settings.offOnSleep;
  $("off-on-quit").checked = settings.offOnQuit;
  for (const [id, key] of behaviorSettings) $(id).checked = !!settings[key];
  $("launch-at-login").checked = loginEnabled;
  $("launch-at-login").disabled = busy || !loginAvailable;
  $("device-detail").textContent = `${settings.count} LEDs · USB HID`;
  $("led-total").textContent = `→ ${settings.count}`;
  $("capture-controls").hidden = !["screen", "music"].includes(settings.mode);
  $("display-controls").hidden = settings.mode !== "screen";
  $("music-controls").hidden = settings.mode !== "music";
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
    settings.mode === "screen" ||
    (settings.mode !== "solid" &&
      (settings.palette !== "mono" || fixedPalette));
  $("speed-controls").hidden =
    settings.mode !== "rainbow" &&
    !(settings.mode === "music" && settings.musicStyle === "wave");
  $("smoothing").disabled = busy || settings.mode === "solid";
  $("screen").disabled = busy || settings.mode !== "screen";
  $("refresh-screens").disabled = busy || settings.mode !== "screen";
  $("capture-help").textContent =
    settings.mode === "screen"
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
}
// Coalesce drag updates: at most one preview IPC in flight, then the newest value.
// Disk writes happen after the user pauses, not on each animation frame.
function previewSettings() {
  previewDirty = true;
  if (!previewPending) {
    previewPending = (async () => {
      while (previewDirty) {
        previewDirty = false;
        await request({ type: "previewSettings", settings: { ...settings } });
      }
    })()
      .catch((error) => {
        notice(String(error), true);
        $("save-state").textContent = "Not saved";
      })
      .finally(() => {
        previewPending = undefined;
      });
  }
  return previewPending;
}
async function save() {
  clearTimeout(saveTimer);
  await previewPending;
  const savingRevision = revision;
  $("save-state").textContent = "Saving…";
  try {
    const result = await request({
      type: "settings",
      settings: { ...settings },
    });
    if (savingRevision === revision) $("save-state").textContent = "Saved";
    return result;
  } catch (error) {
    $("save-state").textContent = "Not saved";
    throw error;
  }
}
function changed() {
  revision++;
  updateUI();
  $("save-state").textContent = "Applying…";
  previewSettings();
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
    const j = (i + s.offset) % shown.length,
      rgb = shown[s.reverse ? shown.length - 1 - j : j];
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
    (!["screen", "music"].includes(value.settings.mode) || value.capturing)
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
  document.querySelectorAll("[data-mode]").forEach((button) => {
    button.setAttribute(
      "aria-label",
      `Start ${labels[button.dataset.mode].toLowerCase()}`,
    );
    button.onclick = () =>
      run(async () => {
        settings.mode = button.dataset.mode;
        updateUI();
        await startLighting();
        if (settings.mode === "screen") await refreshDisplays();
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
          updateUI();
          await startLighting();
        });
    });
  document.querySelectorAll("[data-color]").forEach((button) => {
    button.onclick = () => {
      settings.color = button.dataset.color;
      changed();
    };
  });
  $("color").oninput = (event) => {
    settings.color = event.target.value;
    changed();
  };
  for (const [id, key] of Object.entries(ranges))
    $(id).oninput = (event) => {
      settings[key] = Number(event.target.value);
      changed();
    };
  for (const name of ["count", "sides", "offset", "fps"])
    $(name).onchange = (event) => {
      if (event.target.checkValidity()) {
        settings[name] = Number(event.target.value);
        changed();
      }
    };
  $("palette").onchange = (event) => {
    settings.palette = event.target.value;
    changed();
  };
  for (const [id, key] of [
    ["reverse", "reverse"],
    ["off-on-sleep", "offOnSleep"],
    ["off-on-quit", "offOnQuit"],
    ...behaviorSettings,
  ])
    $(id).onchange = (event) => {
      settings[key] = event.target.checked;
      changed();
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
  if (settings.mode === "screen")
    refreshDisplays().catch((error) => notice(String(error), true));
  window.addEventListener("focus", () =>
    request({ type: "snapshot" }).catch((error) => notice(String(error), true)),
  );
}
init().catch((error) => notice(`Unable to initialize: ${error}`, true));
