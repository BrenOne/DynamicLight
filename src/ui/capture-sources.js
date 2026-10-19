"use strict";
window.CaptureSources = (() => {
  const el = (id) => document.getElementById("source-" + id);
  const keys = ["left", "top", "width", "height"];
  let draft, windows = [], anchor, preview, previewRequest = 0, previewBusy = false;
  const isArea = () =>
    el("kind").value === "region" ||
    (el("kind").value === "window" && el("crop").checked);
  const sameWindow = (a, b) =>
    a.kind === "window" && b.kind === "window" &&
    ["id", "pid", "app", "session"].every((key) => a[key] === b[key]);
  function clearPreview() {
    ++previewRequest;
    preview = undefined;
    anchor = undefined;
    previewBusy = false;
  }
  function fields(area = {}) {
    for (const key of keys)
      el(key).value =
        (area[key] ?? (key === "width" || key === "height" ? 1000 : 0)) / 10;
  }
  function draw() {
    const kind = el("kind").value;
    el("region").hidden = !["region", "window"].includes(kind);
    for (const key of keys) el(key).disabled = kind === "window" && !isArea();
    el("window").hidden = kind !== "window";
    el("snapshot").disabled = previewBusy || el("windows").value === "";
    el("area-help").textContent = kind === "window"
      ? preview
        ? "Still preview. Outline edges determine the LED colors."
        : "No image captured. Percentages are relative to the entire window."
      : "Diagram only. Percentages are relative to the selected display.";
    const canvas = el("preview"),
      css = getComputedStyle(document.documentElement);
    canvas.width = preview?.width || 600;
    canvas.height = preview?.height || 340;
    const ctx = canvas.getContext("2d"),
      width = canvas.width,
      height = canvas.height;
    if (preview) ctx.drawImage(preview, 0, 0);
    else {
      ctx.fillStyle = css.getPropertyValue("--stage");
      ctx.fillRect(0, 0, width, height);
    }
    const [x, y, w, h] = isArea()
      ? keys.map((k) => Number(el(k).value) / 100)
      : [0, 0, 1, 1];
    ctx.strokeStyle = css.getPropertyValue("--accent");
    ctx.lineWidth = 3 * width / (canvas.getBoundingClientRect().width || width);
    ctx.strokeRect(x * width, y * height, w * width, h * height);
  }
  function populateWindows(source) {
    el("windows").replaceChildren(
      new Option("Select a window…", ""),
      ...windows.map((w, i) => new Option(w.name, String(i))),
    );
    const selected = windows.findIndex((w) => sameWindow(w.source, source));
    el("windows").value = selected >= 0 ? String(selected) : "";
    if (source.kind === "window" && selected < 0)
      el("status").textContent = "Refresh windows and select the source again.";
  }
  function selectedWindow() {
    if (el("windows").value === "") throw Error("Select an available window.");
    return structuredClone(windows[Number(el("windows").value)].source);
  }
  function readArea() {
    const area = {};
    for (const key of keys) {
      const field = el(key);
      if (!field.reportValidity())
        throw Error("Use percentages inside the selected source.");
      area[key] = Math.round(Number(field.value) * 10);
    }
    if (area.left + area.width > 1000 || area.top + area.height > 1000)
      throw Error("The rectangle must stay inside the selected source.");
    return area;
  }
  function read() {
    const kind = el("kind").value;
    if (kind === "display") return { kind };
    if (kind === "window") {
      const source = selectedWindow();
      if (el("crop").checked) source.crop = readArea();
      return source;
    }
    return { kind, ...readArea() };
  }
  function sync() {
    const source = settings.captureSource || { kind: "display" };
    el("label").textContent = source.kind === "window"
      ? source.crop ? "Area within application window" : "Application window"
      : source.kind === "region" ? "Selected rectangle" : "Full display";
  }
  function init() {
    el("open").onclick = async () => {
      try {
        await save();
        draft = structuredClone(settings.captureSource || { kind: "display" });
        clearPreview();
        el("status").textContent = "";
        el("kind").value = draft.kind;
        el("crop").checked = !!draft.crop;
        fields(draft.kind === "window" ? draft.crop : draft);
        populateWindows(draft);
        draw();
        el("dialog").showModal();
      } catch (e) {
        notice(String(e), true);
      }
    };
    el("cancel").onclick = () => el("dialog").close();
    el("dialog").addEventListener("close", () => {
      clearPreview();
      draw();
    });
    el("kind").onchange = () => {
      clearPreview();
      el("status").textContent = "";
      fields();
      el("crop").checked = false;
      draw();
    };
    el("windows").onchange = () => {
      clearPreview();
      fields();
      el("crop").checked = false;
      el("status").textContent = "";
      draw();
    };
    el("crop").onchange = draw;
    for (const key of keys) el(key).oninput = draw;
    el("refresh").onclick = async () => {
      el("refresh").disabled = true;
      const selected = el("windows").value === "" ? draft : selectedWindow();
      clearPreview();
      const refreshId = previewRequest;
      draw();
      try {
        const nextWindows = await invoke("capture_windows_list");
        if (!el("dialog").open || refreshId !== previewRequest) return;
        windows = nextWindows;
        populateWindows(selected);
        el("status").textContent = windows.length
          ? "Choose a window. Titles stay on this computer."
          : "No available windows. Open the app to capture, then refresh.";
      } catch (e) {
        if (refreshId === previewRequest) el("status").textContent = String(e);
      } finally {
        el("refresh").disabled = false;
        draw();
      }
    };
    el("snapshot").onclick = async () => {
      let requestId;
      try {
        const source = selectedWindow();
        clearPreview();
        requestId = previewRequest;
        previewBusy = true;
        draw();
        el("status").textContent = "Capturing one preview of the selected window…";
        const result = await invoke("capture_window_preview", { source });
        if (requestId !== previewRequest || !el("dialog").open) return;
        preview = document.createElement("canvas");
        preview.width = result.width;
        preview.height = result.height;
        preview.getContext("2d").putImageData(
          new ImageData(new Uint8ClampedArray(result.rgba), result.width, result.height),
          0, 0,
        );
        el("status").textContent = "Drag over the video to select it. Take another preview if the layout changes.";
      } catch (e) {
        if (requestId === previewRequest) el("status").textContent = String(e);
      } finally {
        if (requestId === previewRequest) {
          previewBusy = false;
          draw();
        }
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
        notice("Capture source saved. Press Start / Retry to begin capturing it.");
      } catch (e) {
        el("status").textContent = String(e);
      }
    };
    const point = (e) => {
      const r = el("preview").getBoundingClientRect();
      return [
        Math.round(Math.max(0, Math.min(1, (e.clientX - r.left) / r.width)) * 1000),
        Math.round(Math.max(0, Math.min(1, (e.clientY - r.top) / r.height)) * 1000),
      ];
    };
    el("preview").onpointerdown = (e) => {
      if (!["region", "window"].includes(el("kind").value)) return;
      if (el("kind").value === "window") {
        if (el("windows").value === "") return;
        el("crop").checked = true;
      }
      anchor = point(e);
      el("preview").setPointerCapture(e.pointerId);
      draw();
    };
    el("preview").onpointermove = (e) => {
      if (!el("preview").hasPointerCapture(e.pointerId) || !anchor) return;
      const p = point(e);
      const left = Math.min(anchor[0], p[0], 999),
        top = Math.min(anchor[1], p[1], 999);
      fields({
        left, top,
        width: Math.max(1, Math.abs(p[0] - anchor[0])),
        height: Math.max(1, Math.abs(p[1] - anchor[1])),
      });
      draw();
    };
    el("preview").onlostpointercapture = () => { anchor = undefined; };
    sync();
  }
  return { init, sync };
})();
