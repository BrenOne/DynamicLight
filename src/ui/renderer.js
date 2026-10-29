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
  settings = {
    ...structuredClone(value.settings),
    ...structuredClone(pendingSettings),
  };
}
const labels = {
  solid: "Color",
  screen: "Screen sync",
  blend: "Screen + audio",
  music: "Music",
  rainbow: "Effects",
  replay: "RGB replay",
};
const effectLabels = {
  rainbow: "Color flow",
  aurora: "Aurora",
  breath: "Breathe",
  comet: "Comet",
  ember: "Ember",
  custom: "Custom",
};
const ranges = {
  brightness: "brightness",
  sensitivity: "sensitivity",
  smoothing: "smoothing",
  speed: "speed",
  "noise-floor": "noiseFloor",
  release: "release",
  "input-gain": "inputGain",
  blend: "blend",
  saturation: "saturation",
  gamma: "gamma",
  "max-brightness": "maxBrightness",
  "transition-limit": "transitionLimit",
  "minimum-glow": "minimumGlow",
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
$("open-compact").onclick = () =>
  run(async () => {
    await save();
    await invoke("compact_window", { operation: "open", pinned: null });
  });
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
  const routineChanged =
    runtime &&
    (value.routines?.sceneRevision !== runtime.routines?.sceneRevision ||
      value.settings.deviceId !== runtime.settings.deviceId);
  const endedCalibration = runtime?.calibrating && !value.calibrating;
  runtime = value;
  if (routineChanged) {
    acceptSettings(value);
    updateUI();
  }
  window.Routines?.sync(value);
  window.ControllerRouting?.sync(value);
  window.Ambient?.sync(value);
  window.Sequences?.sync(value);
  if (endedCalibration && $("calibration-dialog").open) {
    $("calibration-dialog").close();
    settings = value.settings;
    queueMicrotask(updateUI);
  }
  if (!settings) return;
  const connected = value.status.connected;
  const current = value.settings;
  const waiting =
    ["screen", "music", "blend"].includes(current.mode) && !value.capturing;
  const active =
    connected && value.powered && !waiting && current.brightness > 0;
  $("status-dot").classList.toggle(
    "connected",
    LightUI.status(value) === "Sending",
  );
  $("live-dot").classList.toggle("active", active);
  $("status-label").textContent = LightUI.status(value);
  $("connect").textContent = connected ? "Disconnect" : "Reconnect";
  $("connection-message").textContent = value.status.message;
  const running = value.requested;
  $("power").classList.toggle("running", running);
  $("power").setAttribute("aria-pressed", running);
  $("power").querySelector("span").textContent = busy
    ? "Working…"
    : running
      ? "Stop"
      : "Start";
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
          : value.capturing || ["rainbow", "replay"].includes(current.mode)
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
      ? (value.error ? `${value.error} ` : "") +
        "Capture needs a restart. Click Retry to continue."
      : value.error ||
        (value.recovering ? "Reconnecting… Stop cancels recovery." : "");
  if (message !== lastError || (message && $("notice").hidden))
    notice(message, !!value.error && !value.pauseReason);
  lastError = message;
  $("retry").hidden =
    !value.requested ||
    !!value.pauseReason ||
    !(value.error || value.captureRequiresStart || value.recovering);
  $("retry").disabled = busy;
  $("lifecycle-warning").textContent = value.lifecycleWarning || "";
  $("lifecycle-warning").hidden = !value.lifecycleWarning;
  if (value.pauseReason) $("frame-rate").textContent = value.pauseReason;
  else if (value.recovering)
    $("frame-rate").textContent = "Waiting to reconnect";
  draw(value.colors);
  drawSpectrum(value.audioBands);
  updateSceneSelection();
  updateWorkspaceSummary();
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
  for (const name of ["count", "sides", "offset", "sideLeft", "sideTop", "sideRight", "sideBottom", "cornerGap", "fps", "palette"])
    $(name).value = settings[name];
  $("input-gain-value").textContent = settings.inputGain + " dB";
  $("gamma-value").textContent = (settings.gamma / 100).toFixed(2);
  $("transition-limit-value").textContent = settings.transitionLimit
    ? settings.transitionLimit + "% / s"
    : "Off";
  $("exclude-bars").checked = !!settings.excludeBars;
  ["red", "green", "blue"].forEach((c, i) => {
    $(c + "-balance").value = settings.rgbBalance[i];
    $(c + "-balance-value").textContent = settings.rgbBalance[i] + "%";
  });
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
  $("device-detail").textContent =
    `${settings.count} LEDs · ${settings.sides === 3 ? "Three sides" : "Four sides"} · USB HID`;
  $("led-total").textContent = `→ ${settings.count}`;
  $("capture-controls").hidden = !["screen", "music", "blend"].includes(
    settings.mode,
  );
  $("display-controls").hidden = !["screen", "blend"].includes(settings.mode);
  $("music-controls").hidden = !["music", "blend"].includes(settings.mode);
  $("audio-monitor").hidden = $("music-controls").hidden;
  $("effect-controls").hidden = settings.mode !== "rainbow";
  $("palette-controls").hidden = !["music", "rainbow"].includes(settings.mode);
  const fixedPalette =
    settings.mode === "rainbow" &&
    ["rainbow", "ember", "custom"].includes(settings.effect);
  $("palette").disabled = busy || fixedPalette;
  $("palette-help").textContent = fixedPalette
    ? settings.effect === "custom"
      ? "Edit colors in the custom effects editor."
      : "Fixed colors for this effect."
    : "";
  $("color-control").hidden =
    ["screen", "blend", "replay"].includes(settings.mode) ||
    (settings.mode !== "solid" &&
      (settings.palette !== "mono" || fixedPalette));
  $("speed-controls").hidden =
    settings.mode !== "rainbow" &&
    !(settings.mode === "music" && settings.musicStyle === "wave");
  $("smoothing").disabled = busy || settings.mode === "solid";
  $("smoothing-control").hidden = settings.mode === "solid";
  $("screen").disabled = busy || !["screen", "blend"].includes(settings.mode);
  $("refresh-screens").disabled =
    busy || !["screen", "blend"].includes(settings.mode);
  $("capture-help").textContent = ["screen", "blend"].includes(settings.mode)
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
  window.CaptureSources?.sync();
  updateWorkspaceSummary();
}
// Coalesce drag updates: at most one preview IPC in flight, then the newest value.
// Disk writes happen after the user pauses, not on each animation frame.
function previewSettings() {
  if (!previewPending) {
    previewPending = (async () => {
      while (Object.keys(pendingSettings).length) {
        const patch = structuredClone(pendingSettings);
        const value = await request({
          type: "patchSettings",
          patch,
          persist: false,
        });
        for (const [key, sent] of Object.entries(patch)) {
          if (JSON.stringify(pendingSettings[key]) === JSON.stringify(sent))
            delete pendingSettings[key];
        }
        acceptSettings(value);
      }
    })()
      .catch((error) => {
        notice(String(error), true);
        $("save-state").textContent = "Not saved";
        throw error;
      })
      .finally(() => {
        previewPending = undefined;
      });
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
  document
    .querySelectorAll(
      "input:not([id^=automation-]):not([id^=studio-]):not([id^=routine-]):not([id^=controller-]):not([id^=ambient-]):not([id^=extension-]):not([id^=remote-]), select:not([id^=appearance-]):not([id^=studio-]):not([id^=routine-]):not([id^=controller-]):not([id^=ambient-]):not([id^=extension-]):not([id^=remote-])",
    )
    .forEach((input) => {
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
    document
      .querySelectorAll(
        "input:not([id^=automation-]):not([id^=studio-]):not([id^=routine-]):not([id^=controller-]):not([id^=ambient-]):not([id^=extension-]):not([id^=remote-]), select:not([id^=appearance-]):not([id^=studio-]):not([id^=routine-]):not([id^=controller-]):not([id^=ambient-]):not([id^=extension-]):not([id^=remote-])",
      )
      .forEach((input) => {
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
function draw(colors) {
  if (!runtime) return;
  LightUI.monitor($("preview"), colors, runtime.settings, runtime.powered);
  LightUI.strip(
    $("strip-preview"),
    colors,
    runtime.settings,
    runtime.powered,
    runtime.calibrating && runtime.calibrationLed !== null,
  );
}

function drawSpectrum(bands) {
  if ($("music-controls").hidden) return;
  const target = LightUI.surface($("spectrum"));
  if (!target || !bands.length) return;
  const { ctx, width, height } = target;
  const step = width / bands.length;
  const barWidth = Math.max(1, Math.min(16, step * 0.6));
  bands.forEach((v, i) => {
    const h = Math.max(2, Math.min(1, Math.max(0, v)) * (height - 4));
    ctx.fillStyle = `hsl(${155 + i * 2.7} 65% ${50 + v * 16}%)`;
    ctx.globalAlpha = 0.25 + v * 0.75;
    ctx.beginPath();
    ctx.roundRect(
      i * step + (step - barWidth) / 2,
      height - h,
      barWidth,
      h,
      Math.min(3, h / 2),
    );
    ctx.fill();
  });
  ctx.globalAlpha = 1;
}

function isRunning(value) {
  return (
    value?.status.connected &&
    value.powered &&
    (!["screen", "music", "blend"].includes(value.settings.mode) ||
      value.capturing)
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
  updateWorkspaceSummary();
}
const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)");
let thumbnailTimer,
  thumbnailBusy = false,
  lastEffectPreviewKey = "",
  thumbnailsVisible = true,
  thumbnailsFocused = true;
async function refreshEffectPreviews() {
  clearTimeout(thumbnailTimer);
  if (
    !settings ||
    thumbnailBusy ||
    $("effect-controls").hidden ||
    currentView !== "control" ||
    !thumbnailsVisible ||
    !thumbnailsFocused ||
    document.hidden
  )
    return;
  thumbnailBusy = true;
  const key = JSON.stringify(settings);
  const active = reducedMotion.matches
    ? null
    : document.querySelector(
        ".effect-grid button:hover, .effect-grid button:focus-within",
      );
  try {
    if (key === lastEffectPreviewKey && !active) return;
    const frames = await invoke("effect_previews", {
      settings: { ...settings },
      seconds: active ? (performance.now() / 1000) % 86400 : 2,
    });
    if (!$("effect-controls").hidden)
      document.querySelectorAll(".effect-art").forEach((canvas, i) => {
        if (key !== lastEffectPreviewKey || !active || active.contains(canvas))
          drawEffectThumbnail(canvas, frames[i]);
      });
    lastEffectPreviewKey = key;
  } catch (error) {
    notice("Effect preview unavailable: " + String(error), true);
  } finally {
    thumbnailBusy = false;
    if (active || key !== JSON.stringify(settings))
      thumbnailTimer = setTimeout(refreshEffectPreviews, 100);
  }
}
function drawEffectThumbnail(canvas, colors) {
  LightUI.pattern(canvas, colors);
}

let currentView = "control";
const viewTitles = {
  control: "Control",
  scenes: "Scenes",
  device: "Device setup",
  settings: "Settings",
};
function showView(view, focus = true) {
  if (!viewTitles[view]) return;
  currentView = view;
  for (const key of Object.keys(viewTitles))
    $("view-" + key).hidden = key !== view;
  document.querySelectorAll(".app-sidebar [data-view]").forEach((button) => {
    if (button.dataset.view === view)
      button.setAttribute("aria-current", "page");
    else button.removeAttribute("aria-current");
  });
  $("view-title").textContent = viewTitles[view];
  if (focus)
    $("view-" + view)
      .querySelector("h2")
      ?.focus({ preventScroll: true });
  requestAnimationFrame(() => {
    if (runtime) {
      draw(runtime.colors);
      drawSpectrum(runtime.audioBands);
    }
    refreshEffectPreviews();
    refreshScenePreviews();
  });
}
function updateWorkspaceSummary() {
  if (!settings) return;
  const display =
    $("screen").selectedOptions[0]?.textContent || "Primary display";
  const musicNames = {
    spectrum: "Spectrum",
    bass: "Bass pulse",
    vu: "Level meter",
    wave: "Color wave",
    beat: "Beat pulse",
  };
  $("workspace-title").textContent =
    settings.mode === "rainbow"
      ? effectLabels[settings.effect]
      : labels[settings.mode];
  $("inspector-title").textContent =
    settings.mode === "rainbow"
      ? "Effect settings"
      : labels[settings.mode] + " settings";
  $("workspace-subtitle").textContent = ["screen", "blend"].includes(
    settings.mode,
  )
    ? display
    : settings.mode === "music"
      ? musicNames[settings.musicStyle]
      : settings.mode === "solid"
        ? "Fixed color"
        : settings.mode === "replay"
          ? "Offline RGB sequence · controls in Scenes"
          : "Animated pattern";
  $("preview-title").textContent = ["screen", "blend"].includes(settings.mode)
    ? display
    : settings.mode === "solid"
      ? settings.color.toUpperCase()
      : settings.mode === "music"
        ? musicNames[settings.musicStyle]
        : settings.mode === "replay"
          ? "Recorded LED colors"
          : effectLabels[settings.effect];
  $("preview-state").textContent = runtime?.calibrating
    ? "Calibration output"
    : !runtime?.status.connected
      ? "No USB output"
      : LightUI.status(runtime) === "Sending"
        ? "Colors sent to USB"
        : LightUI.status(runtime);
}
function initWorkspace() {
  for (const [index, button] of [
    ...document.querySelectorAll(".app-sidebar [data-view]"),
  ].entries()) {
    button.title = `${viewTitles[button.dataset.view]} (${platform === "macos" ? "⌘" : "Ctrl+"}${index + 1})`;
    button.setAttribute(
      "aria-keyshortcuts",
      `${platform === "macos" ? "Meta" : "Control"}+${index + 1}`,
    );
  }
  document
    .querySelectorAll("[data-view]")
    .forEach(
      (button) => (button.onclick = () => showView(button.dataset.view)),
    );
  document
    .querySelectorAll("[data-close]")
    .forEach(
      (button) => (button.onclick = () => $(button.dataset.close).close()),
    );
  document.querySelectorAll("[data-save-scene]").forEach(
    (button) =>
      (button.onclick = () => {
        $("save-scene-status").textContent = "";
        $("save-scene-dialog").showModal();
        $("profile-name").focus();
      }),
  );
  $("profile-name").addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      event.preventDefault();
      $("profile-save").click();
    }
  });
  $("connection-details").onclick = () => $("connection-dialog").showModal();
  $("preview-toggle").onclick = () => {
    const open = $("view-control").classList.toggle("preview-expanded");
    $("preview-toggle").setAttribute("aria-expanded", String(open));
    $("preview-toggle").querySelector("span").textContent = open
      ? "Hide output preview"
      : "Show output preview";
    if (runtime) draw(runtime.colors);
  };
  const refresh = () => {
    if (runtime) {
      draw(runtime.colors);
      drawSpectrum(runtime.audioBands);
    }
    lastEffectPreviewKey = "";
    lastScenePreviewKey = "";
    refreshEffectPreviews();
    refreshScenePreviews();
  };
  new ResizeObserver(refresh).observe(document.querySelector(".app-views"));
  matchMedia("(prefers-color-scheme: dark)").addEventListener(
    "change",
    refresh,
  );
  for (const event of ["pointerover", "pointerout", "focusin", "focusout"]) {
    $("effect-controls").addEventListener(event, refreshEffectPreviews);
    $("view-scenes").addEventListener(event, refreshScenePreviews);
  }
  document.addEventListener("keydown", (event) => {
    if (
      !(event.ctrlKey || event.metaKey) ||
      event.shiftKey ||
      event.altKey ||
      document.querySelector("dialog[open]")
    )
      return;
    const view = Object.keys(viewTitles)[Number(event.key) - 1];
    if (view) {
      event.preventDefault();
      showView(view);
    }
  });
}

async function init() {
  platform = await invoke("platform_name");
  initWorkspace();
  await LightUI.version();
  if (platform === "linux")
    $("close-behavior").textContent =
      "Closing the window minimizes it and keeps lighting running. Use Quit to exit.";
  await window.__TAURI__.event.listen("lighting", (event) =>
    renderRuntime(event.payload),
  );
  const initial = await request({ type: "snapshot" });
  settings = initial.settings;
  try {
    loginEnabled = await invoke("launch_at_login", { enabled: null });
  } catch (error) {
    loginAvailable = false;
    notice(`Launch at login is unavailable: ${error}`, true);
  }
  await initProfiles();
  window.LibraryTools.init();
  window.Studio.init();
  window.CaptureSources.init();
  window.Sequences.init();
  window.Routines.init();
  window.ControllerRouting.init();
  window.Extensions.init();
  window.PhoneRemote.init();
  window.Ambient.init();
  window.Updates.init({ save, showSettings: () => showView("settings") });
  window.Automation.init();
  initCalibration();
  $("exclude-bars").onchange = (e) => {
    settings.excludeBars = e.target.checked;
    changed(["excludeBars"]);
  };
  ["red", "green", "blue"].forEach((c, i) => {
    $(c + "-balance").oninput = (e) => {
      settings.rgbBalance[i] = Number(e.target.value);
      changed(["rgbBalance"]);
    };
  });
  $("reset-screen").onclick = () => {
    Object.assign(settings, {
      excludeBars: false,
      rgbBalance: [100, 100, 100],
      saturation: 100,
      gamma: 100,
    });
    changed(["excludeBars", "rgbBalance", "saturation", "gamma"]);
  };
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
      if (loginEnabled !== enabled)
        throw new Error(
          "Login startup was not changed. Check your operating system's login-item settings.",
        );
    });
  };
  $("effect-direction").onchange = () => {
    settings.effectReverse = $("effect-direction").value === "reverse";
    changed(["effectReverse"]);
  };
  document.querySelectorAll("[data-mode]").forEach((button) => {
    button.setAttribute(
      "aria-label",
      `Start ${labels[button.dataset.mode].toLowerCase()}`,
    );
    button.onclick = () =>
      run(async () => {
        await save();
        settings = (
          await request({ type: "startMode", mode: button.dataset.mode })
        ).settings;
        updateUI();
        if (["screen", "blend"].includes(settings.mode))
          await refreshDisplays();
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
  for (const name of ["count", "sides", "offset", "sideLeft", "sideTop", "sideRight", "sideBottom", "cornerGap", "fps"])
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
    request({ type: "snapshot" })
      .then((value) => {
        if (!busy && !saveTimer && !previewPending) {
          settings = value.settings;
          updateUI();
        }
      })
      .catch((error) => notice(String(error), true)),
  );
}
function initCalibration() {
  const dialog = $("calibration-dialog");
  let draft,
    edgePreview = false;
  async function step() {
    if (!runtime?.calibrating) return false;
    for (const id of ["cal-count", "cal-led", "cal-offset"])
      if (!$(id).reportValidity()) return false;
    draft.count = Number($("cal-count").value);
    draft.sides = Number($("cal-sides").value);
    draft.offset = Number($("cal-offset").value);
    draft.reverse = $("cal-reverse").checked;
    const led = Math.min(draft.count, Number($("cal-led").value));
    $("cal-led").value = led;
    await request({
      type: "calibrationStep",
      settings: draft,
      led: edgePreview ? null : led - 1,
    });
    $("cal-status").textContent = edgePreview
      ? "Mapped edges: red left · green top · blue right · amber bottom"
      : `Physical LED ${led} of ${draft.count}`;
    return true;
  }
  const safe = (action) => async () => {
    try {
      await action();
    } catch (e) {
      $("cal-status").textContent = String(e);
    }
  };
  $("calibrate").onclick = () =>
    run(async () => {
      await save();
      draft = structuredClone(settings);
      edgePreview = false;
      await request({ type: "calibrationStart" });
      $("cal-count").value = draft.count;
      $("cal-led").value = 1;
      $("cal-sides").value = draft.sides;
      $("cal-offset").value = draft.offset;
      $("cal-reverse").checked = draft.reverse;
      dialog.showModal();
      await step();
    });
  for (const id of ["cal-count", "cal-sides", "cal-offset", "cal-reverse"])
    $(id).onchange = safe(step);
  $("cal-led").onchange = safe(async () => {
    edgePreview = false;
    await step();
  });
  $("cal-prev").onclick = safe(async () => {
    edgePreview = false;
    $("cal-led").value = Math.max(1, Number($("cal-led").value) - 1);
    await step();
  });
  $("cal-next").onclick = safe(async () => {
    edgePreview = false;
    $("cal-led").value = Math.min(draft.count, Number($("cal-led").value) + 1);
    await step();
  });
  $("cal-preview").onclick = safe(async () => {
    edgePreview = true;
    await step();
  });
  async function end(apply, stop = false) {
    const state = await request(
      stop ? { type: "power", on: false } : { type: "calibrationEnd", apply },
    );
    settings = state.settings;
    dialog.close();
    updateUI();
  }
  $("cal-apply").onclick = safe(async () => {
    if (await step()) await end(true);
  });
  $("cal-cancel").onclick = safe(() => end(false));
  $("cal-stop").onclick = safe(() => end(false, true));
  dialog.addEventListener("cancel", (e) => {
    e.preventDefault();
    safe(() => end(false))();
  });
}
let library = { profiles: [] },
  starters = { profiles: [] };
async function initProfiles() {
  library = await invoke("profile_library", { library: null });
  starters = await invoke("starter_scenes");
  renderProfiles();
  $("scene-search").oninput = renderProfiles;
  $("scene-filter").onchange = renderProfiles;
  await window.__TAURI__.event.listen("profiles-changed", async () => {
    try {
      library = await invoke("profile_library", { library: null });
      renderProfiles();
    } catch (error) {
      notice(String(error), true);
    }
  });
  await window.__TAURI__.event.listen("external-settings", async () => {
    try {
      // Finish local field edits, then read the authoritative worker state.
      // Do not replay a full draft over a tray/profile/Quick-controls change.
      await previewPending?.catch(() => {});
      const latest = await request({ type: "snapshot" });
      acceptSettings(latest);
      renderRuntime(latest);
      updateUI();
      if (["screen", "blend"].includes(settings.mode)) await refreshDisplays();
    } catch (error) {
      notice(String(error), true);
    }
  });
  await window.__TAURI__.event.listen("control-error", ({ payload }) =>
    notice(payload, true),
  );
  $("profile-save").onclick = () =>
    run(async () => {
      const name = $("profile-name").value.trim();
      if (!name) {
        $("save-scene-status").textContent = "Enter a scene name.";
        $("profile-name").focus();
        return;
      }
      try {
        await save();
        await editLibrary((next) =>
          next.profiles.push({
            id: crypto.randomUUID(),
            name,
            favorite: true,
            shortcut: "",
            settings: { ...settings, modeSettings: {} },
          }),
        );
        $("profile-name").value = "";
        $("save-scene-dialog").close();
      } catch (error) {
        $("save-scene-status").textContent = String(error);
        throw error;
      }
    });
}
async function editLibrary(edit) {
  const next = structuredClone(library);
  edit(next);
  try {
    library = await invoke("profile_library", { library: next });
    $("profile-message").textContent = "Saved";
    renderProfiles();
  } catch (e) {
    $("profile-message").textContent = String(e);
    throw e;
  }
}
function sceneButton(p, action) {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "profile-apply";
  decorateScene(button, p);
  button.onclick = () =>
    run(async () => {
      await save();
      const state = await action();
      settings = state.settings;
      renderRuntime(state);
      updateUI();
      if (["screen", "blend"].includes(settings.mode)) await refreshDisplays();
    });
  return button;
}
function renderFavorites() {
  const favorites = library.profiles.filter((p) => p.favorite).slice(0, 4);
  document.querySelector(".control-favorites").hidden = !favorites.length;
  $("control-favorites-empty").hidden = !!favorites.length;
  $("control-favorites").replaceChildren(
    ...favorites.map((p) => {
      const button = document.createElement("button");
      button.type = "button";
      button.dataset.favoriteProfile = p.id;
      const canvas = document.createElement("canvas");
      canvas.width = 46;
      canvas.height = 46;
      canvas.setAttribute("aria-hidden", "true");
      const colors =
        p.settings.customPalette?.length && p.settings.palette === "custom"
          ? p.settings.customPalette
          : p.settings.mode === "solid"
            ? [p.settings.color]
            : p.settings.palette === "ocean"
              ? ["#459ddd", "#6ccbd7"]
              : p.settings.palette === "sunset"
                ? ["#eaa07a", "#eac77e"]
                : ["#79cbb9", "#ae96de"];
      const ctx = canvas.getContext("2d");
      colors.forEach((c, i) => {
        ctx.fillStyle = c;
        ctx.fillRect((i * 46) / colors.length, 0, 46 / colors.length + 1, 46);
      });
      button.append(canvas, document.createTextNode(p.name));
      button.onclick = () =>
        run(async () => {
          await save();
          const value = await invoke("apply_profile", { id: p.id });
          settings = value.settings;
          renderRuntime(value);
          updateUI();
          if (["screen", "blend"].includes(settings.mode))
            await refreshDisplays();
        });
      return button;
    }),
  );
}
function editScene(p) {
  const fields = $("scene-editor-fields");
  fields.replaceChildren();
  $("scene-editor-status").textContent = "";
  const field = (label, value) => {
    const wrap = document.createElement("label");
    wrap.textContent = label;
    const input = document.createElement("input");
    input.value = value;
    input.maxLength = 60;
    wrap.append(input);
    fields.append(wrap);
    return input;
  };
  const name = field("Scene name", p.name);
  name.maxLength = 50;
  const shortcut = field("Global shortcut", p.shortcut || "");
  shortcut.placeholder =
    platform === "macos" ? "Command+Shift+1" : "Control+Shift+1";
  const actions = document.createElement("div");
  actions.className = "scene-edit-actions";
  fields.append(actions);
  const mutate = (edit) =>
    editLibrary((next) => {
      const index = next.profiles.findIndex((scene) => scene.id === p.id);
      if (index < 0)
        throw Error(
          "This scene was removed. Close the editor and choose another scene.",
        );
      edit(next, index);
    });
  const action = (text, task, iconName) => {
    const button = document.createElement("button");
    button.type = "button";
    if (iconName) button.append(LightUI.icon(iconName));
    button.append(document.createTextNode(text));
    button.onclick = () =>
      run(async () => {
        try {
          await task();
          $("scene-editor").close();
        } catch (error) {
          $("scene-editor-status").textContent = String(error);
          throw error;
        }
      });
    actions.append(button);
    return button;
  };
  action(
    "Save changes",
    () =>
      mutate((next, i) => {
        next.profiles[i].name = name.value.trim();
        next.profiles[i].shortcut = shortcut.value.trim();
      }),
    "check",
  );
  action(
    "Replace with current settings",
    async () => {
      await save();
      await mutate((next, i) => {
        next.profiles[i].settings = { ...settings, modeSettings: {} };
        next.profiles[i].source = {
          screenId: settings.screenId,
          captureSource: settings.captureSource || { kind: "display" },
        };
      });
    },
    "refresh-cw",
  );
  action(
    "Duplicate",
    () =>
      mutate((next, i) => {
        next.profiles.splice(i + 1, 0, {
          ...structuredClone(next.profiles[i]),
          id: crypto.randomUUID(),
          name: (next.profiles[i].name + " copy").slice(0, 50),
          shortcut: "",
        });
      }),
    "copy",
  );
  const index = library.profiles.findIndex((scene) => scene.id === p.id);
  action(
    "Move up",
    () =>
      mutate((next, i) => {
        if (i > 0)
          [next.profiles[i - 1], next.profiles[i]] = [
            next.profiles[i],
            next.profiles[i - 1],
          ];
      }),
    "arrow-up",
  ).disabled = index === 0;
  action(
    "Move down",
    () =>
      mutate((next, i) => {
        if (i < next.profiles.length - 1)
          [next.profiles[i + 1], next.profiles[i]] = [
            next.profiles[i],
            next.profiles[i + 1],
          ];
      }),
    "arrow-down",
  ).disabled = index === library.profiles.length - 1;
  action(
    "Delete scene",
    () => mutate((next, i) => next.profiles.splice(i, 1)),
    "trash-2",
  ).classList.add("danger");
  $("scene-editor").showModal();
  name.focus();
}
function renderProfiles() {
  window.LibraryTools?.sync();
  lastScenePreviewKey = "";
  $("profile-list").replaceChildren();
  for (const p of library.profiles) {
    if (!sceneVisible(p, false)) continue;
    const row = document.createElement("div");
    row.className = "profile-row scene-card";
    row.setAttribute("role", "group");
    row.setAttribute("aria-label", p.name);
    row.append(sceneButton(p, () => invoke("apply_profile", { id: p.id })));
    const actions = document.createElement("div");
    actions.className = "scene-actions";
    const favorite = document.createElement("button");
    favorite.type = "button";
    favorite.className = "scene-favorite text-button";
    favorite.append(LightUI.icon("star"), document.createTextNode("Favorite"));
    favorite.setAttribute("aria-label", "Favorite " + p.name);
    favorite.setAttribute("aria-pressed", String(p.favorite));
    favorite.onclick = () =>
      run(() =>
        editLibrary((next) => {
          const current = next.profiles.find((scene) => scene.id === p.id);
          if (current) current.favorite = !current.favorite;
        }),
      );
    const edit = document.createElement("button");
    edit.type = "button";
    edit.className = "text-button";
    edit.append(LightUI.icon("ellipsis"), document.createTextNode("Edit"));
    edit.setAttribute("aria-label", "Edit " + p.name);
    edit.onclick = () => editScene(p);
    actions.append(favorite, edit);
    row.append(actions);
    $("profile-list").append(row);
  }
  $("profile-message").textContent = library.profiles.length
    ? ""
    : "Save your current settings to create a scene.";
  $("starter-list").replaceChildren();
  for (const p of starters.profiles) {
    if (!sceneVisible(p, true)) continue;
    const row = document.createElement("div");
    row.className = "scene-card starter-card";
    row.setAttribute("role", "group");
    row.setAttribute("aria-label", p.name);
    row.append(sceneButton(p, () => invoke("apply_starter", { id: p.id })));
    const actions = document.createElement("div");
    actions.className = "scene-actions";
    const copy = document.createElement("button");
    copy.type = "button";
    copy.className = "text-button";
    copy.append(LightUI.icon("plus"), document.createTextNode("Save a copy"));
    copy.onclick = () =>
      run(() =>
        editLibrary((next) =>
          next.profiles.push({
            ...structuredClone(p),
            id: crypto.randomUUID(),
            favorite: true,
          }),
        ),
      );
    actions.append(copy);
    row.append(actions);
    $("starter-list").append(row);
  }
  $("saved-scenes-heading").hidden = !$("profile-list").children.length;
  $("starter-scenes-heading").hidden = !$("starter-list").children.length;
  $("scene-empty").hidden = !!document.querySelector(".scene-card");
  renderFavorites();
  updateSceneSelection();
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
  controls.forEach((c) => (c.disabled = true));
  $("diagnostic-status").textContent = "";
  try {
    await action();
  } catch (e) {
    $("diagnostic-status").textContent = String(e);
  } finally {
    controls.forEach((c) => (c.disabled = false));
  }
}
$("diagnostic-preview").onclick = () => {
  $("diagnostic-dialog").showModal();
  diagnosticAction(() => refreshDiagnostics());
};
$("diagnostic-close").onclick = () => $("diagnostic-dialog").close();
$("diagnostic-enabled").onchange = () =>
  diagnosticAction(async () => {
    try {
      await refreshDiagnostics($("diagnostic-enabled").checked);
    } catch (e) {
      $("diagnostic-enabled").checked = !$("diagnostic-enabled").checked;
      throw e;
    }
  });
$("diagnostic-clear").onclick = () =>
  diagnosticAction(async () => {
    await refreshDiagnostics(null, true);
    $("diagnostic-status").textContent = "History cleared.";
  });
$("diagnostic-copy").onclick = () =>
  diagnosticAction(async () => {
    try {
      await navigator.clipboard.writeText($("diagnostic-text").value);
      $("diagnostic-status").textContent = "Copied.";
    } catch {
      $("diagnostic-text").focus();
      $("diagnostic-text").select();
      $("diagnostic-status").textContent =
        "Select Copy from the text menu, or press Ctrl+C / ⌘C.";
    }
  });
$("diagnostic-export").onclick = () =>
  diagnosticAction(async () => {
    const path = await invoke("export_diagnostics");
    $("diagnostic-status").textContent = "Saved to " + path;
  });

let sceneTimer,
  scenePreviewBusy = false,
  scenePreviewDirty = false,
  lastScenePreviewKey = "";
function sceneVisible(p, starter) {
  const filter = $("scene-filter").value;
  const query = $("scene-search").value.trim().toLowerCase();
  return (
    !(
      (filter === "favorites" && !p.favorite) ||
      (filter === "saved" && starter) ||
      (filter === "starters" && !starter)
    ) && (p.name + " " + labels[p.settings.mode]).toLowerCase().includes(query)
  );
}
function decorateScene(button, p) {
  button.scene = p;
  const canvas = document.createElement("canvas");
  canvas.className = "scene-art";
  canvas.width = 240;
  canvas.height = 150;
  canvas.setAttribute("aria-hidden", "true");
  const title = document.createElement("strong");
  title.textContent = p.name;
  const meta = document.createElement("small");
  const s = p.settings;
  meta.textContent =
    (s.mode === "rainbow" ? effectLabels[s.effect] : labels[s.mode]) +
    " · " +
    s.brightness +
    "%" +
    (s.mode === "rainbow" ? " · " + s.speed + "% speed" : "");
  const status = document.createElement("span");
  status.className = "scene-tag";
  status.textContent = "Example preview";
  button.append(canvas, title, meta, status);
  button.setAttribute("aria-label", "Start " + p.name);
}
async function refreshScenePreviews() {
  clearTimeout(sceneTimer);
  if (scenePreviewBusy) {
    scenePreviewDirty = true;
    return;
  }
  if (currentView !== "scenes" || document.hidden || !thumbnailsFocused) return;
  scenePreviewDirty = false;
  scenePreviewBusy = true;
  try {
    const buttons = [
      ...document.querySelectorAll(".scene-card .profile-apply"),
    ].filter((b) => {
      const r = b.getBoundingClientRect();
      return r.bottom > 0 && r.top < innerHeight;
    });
    const key = JSON.stringify(
      buttons.map((b) => [b.scene.id, b.scene.settings]),
    );
    const active = reducedMotion.matches
      ? null
      : document.querySelector(
          ".scene-card .profile-apply:hover, .scene-card .profile-apply:focus-within",
        );
    if (key === lastScenePreviewKey && !active) return;
    for (let i = 0; i < buttons.length; i += 8) {
      const chunk = buttons.slice(i, i + 8);
      const frames = await invoke("scene_previews", {
        settings: chunk.map((b) => b.scene.settings),
        seconds: active ? (performance.now() / 1000) % 86400 : 2,
      });
      chunk.forEach((b, j) => {
        if (
          b.isConnected &&
          (key !== lastScenePreviewKey || !active || active === b)
        )
          drawEffectThumbnail(b.querySelector("canvas"), frames[j]);
      });
    }
    lastScenePreviewKey = key;
    updateSceneSelection();
  } catch (e) {
    $("profile-message").textContent =
      "Scene previews unavailable: " + String(e);
  } finally {
    scenePreviewBusy = false;
    if (scenePreviewDirty) lastScenePreviewKey = "";
    if (
      scenePreviewDirty ||
      (!reducedMotion.matches &&
        document.querySelector(
          ".scene-card .profile-apply:hover, .scene-card .profile-apply:focus-within",
        ))
    )
      sceneTimer = setTimeout(refreshScenePreviews, 150);
  }
}
function updateSceneSelection() {
  if (!runtime) return;
  for (const button of document.querySelectorAll(
    ".scene-card .profile-apply",
  )) {
    const match = LightUI.matchesScene(button.scene, runtime);
    button.classList.toggle("scene-current", match);
    button.querySelector(".scene-tag").textContent = match
      ? "Current settings · example"
      : "Example preview";
  }
  for (const button of document.querySelectorAll("[data-favorite-profile]")) {
    const profile = library.profiles.find(
      (p) => p.id === button.dataset.favoriteProfile,
    );
    if (profile)
      button.setAttribute(
        "aria-pressed",
        String(LightUI.matchesScene(profile, runtime)),
      );
  }
}

document.addEventListener("scroll", refreshScenePreviews, {
  passive: true,
  capture: true,
});
window.addEventListener("focus", refreshScenePreviews);
document.addEventListener("visibilitychange", refreshScenePreviews);
reducedMotion.addEventListener("change", refreshScenePreviews);
