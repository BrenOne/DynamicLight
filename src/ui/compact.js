"use strict";
const $ = (id) => document.getElementById(id);
const { invoke } = window.__TAURI__.core;
const names = {
  solid: "Color",
  screen: "Screen",
  music: "Music",
  blend: "Screen + music",
  rainbow: "Effects",
};
let state,
  favorites = [],
  busy = false,
  brightnessTimer,
  editingBrightness = false,
  queue = Promise.resolve(),
  actionError = "";
let brightnessPreview,
  brightnessDirty = false,
  previewFailed = false,
  brightnessRevision = 0;
function serialized(task) {
  const next = queue.catch(() => {}).then(task);
  queue = next;
  return next;
}
function statusText(id, text) {
  if ($(id).textContent !== text) $(id).textContent = text;
}
function render(s) {
  state = s;
  $("quick-mode").textContent = names[s.settings.mode];
  statusText("quick-status", s.pauseReason || LightUI.status(s));
  $("quick-power").textContent = s.requested ? "Stop" : "Start";
  $("quick-power").classList.toggle("running", s.requested);
  $("quick-power").setAttribute("aria-pressed", String(s.requested));
  $("quick-power").disabled = busy;
  $("quick-pin").disabled = busy;
  $("open-studio").disabled = busy;
  $("quick-brightness").disabled = busy || s.calibrating;
  if (!editingBrightness) {
    $("quick-brightness").value = s.settings.brightness;
    $("quick-value").textContent = s.settings.brightness + "%";
  }
  statusText("quick-error", actionError || s.error);
  $("quick-error").hidden = !$("quick-error").textContent;
  $("quick-retry").hidden =
    !s.requested ||
    !!s.pauseReason ||
    !(s.error || s.captureRequiresStart || s.recovering);
  $("quick-retry").disabled = busy;
  $("quick-delivery").textContent = !s.status.connected
    ? "USB disconnected"
    : LightUI.status(s) !== "Sending"
      ? "USB idle"
      : s.settings.mode === "solid"
        ? "Color sent · USB"
        : `${s.fps} frames/s · USB`;
  LightUI.strip(
    $("quick-strip"),
    s.colors,
    s.settings,
    s.powered,
    s.calibrating && s.calibrationLed !== null,
  );
  LightUI.pattern(
    $("quick-pattern"),
    s.powered ? s.colors : s.colors.map(() => [0, 0, 0]),
  );
  document.querySelectorAll("[data-quick-mode]").forEach((button) => {
    const selected = button.dataset.quickMode === s.settings.mode;
    button.classList.toggle("selected", selected);
    button.setAttribute("aria-pressed", String(selected));
    button.disabled = busy || s.calibrating;
  });
  document.querySelectorAll("#quick-favorites button").forEach((button, i) => {
    button.disabled = busy || s.calibrating;
    button.setAttribute(
      "aria-pressed",
      String(LightUI.matchesScene(favorites[i], s)),
    );
  });
}
async function action(task) {
  if (busy) return;
  busy = true;
  actionError = "";
  if (state) render(state);
  try {
    await flushBrightness();
    const s = await serialized(task);
    if (s?.settings) render(s);
  } catch (e) {
    actionError = String(e);
  } finally {
    busy = false;
    if (state) render(state);
  }
}
async function flushBrightness() {
  clearTimeout(brightnessTimer);
  if (!editingBrightness) return;
  await brightnessPreview;
  const commitRevision = brightnessRevision;
  const brightness = previewFailed ? +$("quick-brightness").value : null;
  // Preview already applied the edit. Persist current worker state so a later
  // Studio/tray edit is not overwritten by this delayed commit.
  const next = await serialized(() =>
    invoke("compact_control", { brightness, toggle: false }),
  );
  if (commitRevision === brightnessRevision) editingBrightness = false;
  previewFailed = false;
  actionError = "";
  render(next);
}
function previewBrightness() {
  brightnessDirty = true;
  if (!brightnessPreview)
    brightnessPreview = (async () => {
      while (brightnessDirty) {
        brightnessDirty = false;
        const brightness = +$("quick-brightness").value;
        render(
          await serialized(() =>
            invoke("compact_control", {
              brightness,
              toggle: false,
              preview: true,
            }),
          ),
        );
        previewFailed = false;
      }
    })()
      .catch((e) => {
        previewFailed = true;
        actionError = String(e);
        render(state);
      })
      .finally(() => {
        brightnessPreview = null;
      });
}
$("quick-brightness").oninput = () => {
  brightnessRevision++;
  editingBrightness = true;
  $("quick-value").textContent = $("quick-brightness").value + "%";
  previewBrightness();
  clearTimeout(brightnessTimer);
  brightnessTimer = setTimeout(
    () =>
      flushBrightness().catch((e) => {
        actionError = String(e);
        render(state);
      }),
    500,
  );
};
$("quick-brightness").onchange = () =>
  flushBrightness().catch((e) => {
    actionError = String(e);
    render(state);
  });
$("quick-power").onclick = () =>
  action(() => invoke("compact_control", { brightness: null, toggle: true }));
$("quick-retry").onclick = () =>
  action(() => invoke("control", { action: { type: "start" } }));
document.querySelectorAll("[data-quick-mode]").forEach((button) => {
  button.setAttribute(
    "aria-label",
    "Start " + names[button.dataset.quickMode].toLowerCase(),
  );
  button.onclick = () =>
    action(() =>
      invoke("compact_control", {
        mode: button.dataset.quickMode,
        brightness: null,
        toggle: false,
      }),
    );
});
$("open-studio").onclick = () =>
  action(() => invoke("compact_window", { operation: "studio", pinned: null }));
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape")
    action(() => invoke("compact_window", { operation: "hide", pinned: null }));
});
$("quick-pin").onchange = () =>
  action(async () => {
    try {
      $("quick-pin").checked = await invoke("compact_window", {
        operation: "pin",
        pinned: $("quick-pin").checked,
      });
    } catch (e) {
      $("quick-pin").checked = !$("quick-pin").checked;
      throw e;
    }
  });
async function loadFavorites() {
  const library = await invoke("profile_library", { library: null });
  favorites = library.profiles.filter((p) => p.favorite);
  $("quick-favorites").replaceChildren();
  $("quick-empty").hidden = !!favorites.length;
  for (const p of favorites) {
    const button = document.createElement("button");
    button.className = "secondary";
    button.textContent = p.name;
    button.onclick = () => action(() => invoke("apply_profile", { id: p.id }));
    $("quick-favorites").append(button);
  }
  if (state) render(state);
}
async function poll() {
  try {
    if (!document.hidden && !busy)
      render(
        await serialized(() =>
          invoke("control", { action: { type: "snapshot" } }),
        ),
      );
  } catch (e) {
    actionError = String(e);
    if (state) render(state);
  } finally {
    setTimeout(poll, 500);
  }
}
(async () => {
  await LightUI.version();
  const refresh = () => {
    if (state) render(state);
  };
  new ResizeObserver(refresh).observe(
    document.querySelector(".quick-controls"),
  );
  matchMedia("(prefers-color-scheme: dark)").addEventListener(
    "change",
    refresh,
  );
  await window.__TAURI__.event.listen("profiles-changed", () =>
    loadFavorites().catch((e) => {
      actionError = String(e);
    }),
  );
  await window.__TAURI__.event.listen("control-error", ({ payload }) => {
    actionError = String(payload);
    if (state) render(state);
  });
  $("quick-pin").checked = await invoke("compact_window", {
    operation: "get",
    pinned: null,
  });
  await loadFavorites();
  render(await invoke("control", { action: { type: "snapshot" } }));
})()
  .catch((e) => {
    $("quick-error").textContent = String(e);
    $("quick-error").hidden = false;
  })
  .finally(poll);
