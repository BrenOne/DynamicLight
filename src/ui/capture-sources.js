"use strict";
window.CaptureSources = (() => {
  const el = (id) => document.getElementById("source-" + id);
  let draft,
    windows = [],
    anchor;
  function draw() {
    const kind = el("kind").value;
    el("region").hidden = kind !== "region";
    el("window").hidden = kind !== "window";
    const canvas = el("preview"),
      ctx = canvas.getContext("2d"),
      css = getComputedStyle(document.documentElement);
    ctx.fillStyle = css.getPropertyValue("--stage");
    ctx.fillRect(0, 0, 600, 340);
    ctx.strokeStyle = css.getPropertyValue("--muted");
    ctx.strokeRect(1, 1, 598, 338);
    const values = ["left", "top", "width", "height"].map(
      (k) => Number(el(k).value) / 100,
    );
    const [x, y, w, h] = kind === "region" ? values : [0, 0, 1, 1];
    ctx.strokeStyle = css.getPropertyValue("--accent");
    ctx.lineWidth = 4;
    ctx.strokeRect(x * 600, y * 340, w * 600, h * 340);
  }
  function render() {
    el("kind").value = draft.kind;
    for (const key of ["left", "top", "width", "height"])
      el(key).value =
        (draft[key] ?? (key === "width" || key === "height" ? 1000 : 0)) / 10;
    el("windows").replaceChildren(
      new Option("Select a window…", ""),
      ...windows.map((w, i) => new Option(w.name, String(i))),
    );
    const selected = windows.findIndex(
      (w) => JSON.stringify(w.source) === JSON.stringify(draft),
    );
    el("windows").value = selected >= 0 ? String(selected) : "";
    if (draft.kind === "window" && selected < 0)
      el("status").textContent = "Refresh windows and select the source again.";
    draw();
  }
  function read() {
    const kind = el("kind").value;
    if (kind === "display") return { kind };
    if (kind === "window") {
      if (el("windows").value === "")
        throw Error("Select an available window.");
      return structuredClone(windows[Number(el("windows").value)].source);
    }
    const source = { kind };
    for (const key of ["left", "top", "width", "height"]) {
      const field = el(key);
      if (!field.reportValidity())
        throw Error("Use percentages inside the selected display.");
      source[key] = Math.round(Number(field.value) * 10);
    }
    if (source.left + source.width > 1000 || source.top + source.height > 1000)
      throw Error("The rectangle must stay inside the selected display.");
    return source;
  }
  function sync() {
    const source = settings.captureSource || { kind: "display" };
    el("label").textContent =
      source.kind === "window"
        ? "Application window"
        : source.kind === "region"
          ? "Selected rectangle"
          : "Full display";
  }
  function init() {
    el("open").onclick = async () => {
      try {
        await save();
        draft = structuredClone(settings.captureSource || { kind: "display" });
        el("status").textContent = "";
        render();
        el("dialog").showModal();
      } catch (e) {
        notice(String(e), true);
      }
    };
    el("cancel").onclick = () => el("dialog").close();
    el("kind").onchange = draw;
    for (const key of ["left", "top", "width", "height"])
      el(key).oninput = draw;
    el("refresh").onclick = async () => {
      el("refresh").disabled = true;
      try {
        windows = await invoke("capture_windows_list");
        draft = { kind: el("kind").value };
        render();
        el("status").textContent = windows.length
          ? "Choose a window. Titles stay on this computer."
          : "No available windows. Open the app to capture, then refresh.";
      } catch (e) {
        el("status").textContent = String(e);
      } finally {
        el("refresh").disabled = false;
      }
    };
    el("apply").onclick = async () => {
      try {
        const captureSource = read();
        const result = await request({
          type: "patchSettings",
          patch: { captureSource },
          persist: true,
        });
        acceptSettings(result);
        updateUI();
        el("dialog").close();
        notice(
          "Capture source saved. Press Start / Retry to begin capturing it.",
        );
      } catch (e) {
        el("status").textContent = String(e);
      }
    };
    const point = (e) => {
      const r = el("preview").getBoundingClientRect();
      return [
        Math.round(
          Math.max(0, Math.min(1, (e.clientX - r.left) / r.width)) * 1000,
        ),
        Math.round(
          Math.max(0, Math.min(1, (e.clientY - r.top) / r.height)) * 1000,
        ),
      ];
    };
    el("preview").onpointerdown = (e) => {
      if (el("kind").value !== "region") return;
      anchor = point(e);
      el("preview").setPointerCapture(e.pointerId);
    };
    el("preview").onpointermove = (e) => {
      if (!el("preview").hasPointerCapture(e.pointerId) || !anchor) return;
      const p = point(e);
      const left = Math.min(anchor[0], p[0], 999),
        top = Math.min(anchor[1], p[1], 999);
      const values = {
        left,
        top,
        width: Math.max(1, Math.abs(p[0] - anchor[0])),
        height: Math.max(1, Math.abs(p[1] - anchor[1])),
      };
      for (const [key, value] of Object.entries(values))
        el(key).value = value / 10;
      draw();
    };
    sync();
  }
  return { init, sync };
})();
