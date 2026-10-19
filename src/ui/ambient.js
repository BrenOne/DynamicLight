"use strict";
window.Ambient = (() => {
  const el = (id) => document.getElementById("ambient-" + id);
  let working = false;
  function sync(value) {
    const s = value?.ambient;
    if (!s) return;
    el("status").textContent =
      `${s.config.enabled ? "Enabled" : "Disabled"} · ${s.manualOverride ? "manual override" : s.available ? `${s.lux.toFixed(1)} lux · ${s.brightness}% sensor level` : "fixed fallback"} · ${s.message}`;
  }
  async function load() {
    const [state, sources] = await Promise.all([
      invoke("ambient_config", { config: null }),
      invoke("ambient_sources"),
    ]);
    const c = state.config;
    el("enabled").checked = c.enabled;
    el("source").replaceChildren();
    for (const s of sources.sources)
      el("source").add(
        new Option(`${s.name}${s.available ? "" : " (unavailable)"}`, s.id),
      );
    if (!sources.sources.some((s) => s.id === c.source))
      el("source").add(new Option(c.source + " (unavailable)", c.source));
    el("source").value = c.source;
    for (const key of [
      "lowLux",
      "highLux",
      "minimum",
      "maximum",
      "fallback",
      "smoothing",
    ])
      el(key).value = c[key];
    el("file").textContent = sources.externalFile;
    el("availability").textContent = sources.sources
      .map((s) => `${s.name}: ${s.message}`)
      .join(" · ");
    sync({ ambient: state });
  }
  async function task(fn) {
    if (working) return;
    working = true;
    for (const b of el("panel").querySelectorAll("button")) b.disabled = true;
    try {
      await fn();
    } catch (e) {
      el("status").textContent = String(e);
    } finally {
      working = false;
      for (const b of el("panel").querySelectorAll("button"))
        b.disabled = false;
    }
  }
  function init() {
    el("refresh").onclick = () => task(load);
    el("save").onclick = () =>
      task(async () => {
        const invalid = el("panel").querySelector("input:invalid");
        if (invalid) {
          invalid.reportValidity();
          return;
        }
        const config = {
          enabled: el("enabled").checked,
          source: el("source").value,
        };
        for (const key of [
          "lowLux",
          "highLux",
          "minimum",
          "maximum",
          "fallback",
          "smoothing",
        ])
          config[key] = Number(el(key).value);
        sync({ ambient: await invoke("ambient_config", { config }) });
      });
    el("resume").onclick = () => task(() => request({ type: "ambientResume" }));
  }
  return { init, load, sync };
})();
