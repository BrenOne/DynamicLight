"use strict";
window.Sequences = (() => {
  const el = (id) => document.getElementById("sequence-" + id);
  let lastClip = "",
    working = false;
  function sync(value = runtime) {
    const state = value?.sequence || { recording: false, clip: null },
      clip = state.clip;
    el("record-state").textContent = state.recording
      ? `Recording ${(state.elapsedMs / 1000).toFixed(1)} s · ${state.recordedFrames} RGB frames`
      : "Recording stopped.";
    el("record").disabled =
      working || state.recording || value?.settings.mode === "replay";
    el("stop-record").disabled = working || !state.recording;
    el("play").disabled = working || !clip || state.recording;
    el("export").disabled = working || !clip;
    el("file").disabled = working || state.recording;
    el("clip").textContent = clip
      ? `${clip.count} LEDs · ${clip.sides} sides · ${(clip.durationMs / 1000).toFixed(2)} seconds · ${clip.frames} frames`
      : "No RGB sequence loaded.";
    if (clip && clip.id !== lastClip) {
      lastClip = clip.id;
      const config =
        value.settings.replay?.clip === clip.id ? value.settings.replay : {};
      el("start").value = (config.startMs || 0) / 1000;
      el("end").value = (config.endMs || clip.durationMs) / 1000;
      el("speed").value = config.speed || 100;
      el("loop").checked = config.looping ?? true;
      el("remap").checked = !!config.remap;
      el("level").value =
        value.settings.mode === "replay" ? value.settings.brightness : 100;
    }
    el("mapping").textContent =
      clip &&
      (clip.count !== value.settings.count ||
        clip.sides !== value.settings.sides)
        ? `Recorded layout differs from this ${value.settings.count}-LED / ${value.settings.sides}-side strip. Explicit proportional remapping is required.`
        : "Layout matches the loaded sequence.";
  }
  async function task(fn) {
    if (working) return;
    working = true;
    sync();
    try {
      await fn();
    } catch (e) {
      el("status").textContent = String(e);
    } finally {
      working = false;
      sync();
    }
  }
  function init() {
    el("open").onclick = () => {
      el("status").textContent = "";
      sync();
      el("dialog").showModal();
    };
    el("close").onclick = () => el("dialog").close();
    el("record").onclick = () =>
      task(async () => {
        if (!el("seconds").reportValidity()) return;
        await save();
        await request({
          type: "recordStart",
          seconds: Number(el("seconds").value),
        });
        el("status").textContent =
          "Recording generated LED colors only. The dialog may be closed while recording.";
      });
    el("stop-record").onclick = () =>
      task(async () => {
        await request({ type: "recordStop" });
        el("status").textContent =
          "Recording saved locally. Export to keep a separate sequence file.";
      });
    el("export").onclick = () =>
      task(async () => {
        el("status").textContent =
          "Saved to " + (await invoke("export_sequence"));
      });
    el("file").onchange = (e) =>
      task(async () => {
        const file = e.target.files[0];
        if (!file) return;
        try {
          if (file.size > 10 * 1024 * 1024)
            throw Error("RGB sequence files must be at most 10 MiB.");
          const value = await invoke("import_sequence", {
            text: await file.text(),
          });
          acceptSettings(value);
          renderRuntime(value);
          updateUI();
          el("status").textContent =
            "Sequence imported. Play starts offline lighting; import does not start capture.";
        } finally {
          e.target.value = "";
        }
      });
    el("play").onclick = () =>
      task(async () => {
        if (
          !["start", "end", "speed", "level"].every((k) =>
            el(k).reportValidity(),
          )
        )
          return;
        const clip = runtime.sequence?.clip;
        if (!clip) throw Error("Import or record a sequence first.");
        const replay = {
          clip: clip.id,
          startMs: Math.round(Number(el("start").value) * 1000),
          endMs: Math.round(Number(el("end").value) * 1000),
          speed: Number(el("speed").value),
          looping: el("loop").checked,
          remap: el("remap").checked,
        };
        await save();
        const value = await request({
          type: "patchSettings",
          patch: {
            mode: "replay",
            replay,
            brightness: Number(el("level").value),
            smoothing: 0,
          },
          persist: true,
        });
        acceptSettings(value);
        updateUI();
        await request({ type: "start" });
        el("status").textContent =
          "Offline replay started. Capture is stopped; ordinary Stop and comfort limits still apply.";
      });
    el("stop").onclick = () =>
      task(async () => {
        await request({ type: "power", on: false });
        el("status").textContent = "Lighting stopped.";
      });
    sync();
  }
  return { init, sync };
})();
