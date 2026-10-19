"use strict";
window.Routines = (() => {
  const el = (id) => document.getElementById("routine-" + id);
  let config,
    profiles = [],
    working = false;
  const minute = (value) => {
    const [h, m] = value.split(":").map(Number);
    return h * 60 + m;
  };
  const time = (value) =>
    `${String(Math.floor(value / 60)).padStart(2, "0")}:${String(value % 60).padStart(2, "0")}`;
  async function task(fn) {
    if (working) return;
    working = true;
    el("dialog")
      .querySelectorAll("button:not(#routine-close)")
      .forEach((button) => {
        button.disabled = true;
      });
    try {
      await fn();
    } catch (e) {
      el("message").textContent = String(e);
    } finally {
      working = false;
      el("dialog")
        .querySelectorAll("button:not(#routine-close)")
        .forEach((button) => {
          button.disabled = false;
        });
      sync(runtime);
    }
  }
  function sync(value) {
    const s = value?.routines;
    if (!s) return;
    el("status").textContent = [s.next, s.activeRule, s.message]
      .filter(Boolean)
      .join(" · ");
    el("timer-status").textContent = s.timerPhase
      ? `${s.timerPhase} · cycle ${s.cycle} · ${Math.floor(s.remainingSeconds / 60)}:${String(s.remainingSeconds % 60).padStart(2, "0")} remaining${s.timerPaused ? " · paused" : ""}`
      : "Timer stopped";
    el("pause-timer").textContent = s.timerPaused
      ? "Resume timer"
      : "Pause timer";
    for (const id of ["pause-timer", "skip", "cancel"])
      el(id).disabled = working || !s.timerPhase;
  }
  function draw() {
    el("paused").checked = config.paused;
    el("apps-enabled").checked = config.appsEnabled;
    el("quiet-enabled").checked = config.quietEnabled;
    el("quiet-start").value = time(config.quietStart);
    el("quiet-end").value = time(config.quietEnd);
    for (const [id, key] of [
      ["focus", "focusMinutes"],
      ["break", "breakMinutes"],
      ["cycles", "cycles"],
    ])
      el(id).value = config[key];
    for (const key of ["manualCue", "focusCue", "breakCue"]) {
      el(key + "-enabled").checked = config[key].enabled;
      el(key + "-color").value = config[key].color;
      el(key + "-seconds").value = config[key].seconds;
    }
    rows();
  }
  function rows() {
    for (const [id, items] of [
      ["schedules", config.schedules],
      ["rules", config.appRules],
    ]) {
      el(id).replaceChildren();
      items.forEach((item, i) => {
        const row = document.createElement("div");
        row.className = "input-row";
        const label = document.createElement("span");
        label.textContent =
          id === "rules"
            ? `${i + 1}. ${item.app} → ${item.scene.name}`
            : `${item.enabled ? "On" : "Off"} · ${item.days.map((d) => ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"][d]).join(", ")} ${time(item.minute)} · ${item.scene.name} · ${item.transition}${item.transition !== "immediate" ? ` ${item.seconds}s` : ""}`;
        row.append(label);
        const button = (text, fn) => {
          const b = document.createElement("button");
          b.type = "button";
          b.textContent = text;
          b.onclick = fn;
          row.append(b);
        };
        if (id === "schedules")
          button(item.enabled ? "Disable" : "Enable", () => {
            item.enabled = !item.enabled;
            rows();
          });
        if (i > 0)
          button("Move up", () => {
            [items[i - 1], items[i]] = [items[i], items[i - 1]];
            rows();
          });
        button("Remove", () => {
          items.splice(i, 1);
          rows();
        });
        el(id).append(row);
      });
    }
  }
  function read() {
    if (!el("dialog").querySelectorAll("input:invalid").length) {
      config.paused = el("paused").checked;
      config.appsEnabled = el("apps-enabled").checked;
      config.quietEnabled = el("quiet-enabled").checked;
      config.quietStart = minute(el("quiet-start").value);
      config.quietEnd = minute(el("quiet-end").value);
      config.focusMinutes = Number(el("focus").value);
      config.breakMinutes = Number(el("break").value);
      config.cycles = Number(el("cycles").value);
      for (const key of ["manualCue", "focusCue", "breakCue"])
        config[key] = {
          enabled: el(key + "-enabled").checked,
          color: el(key + "-color").value,
          seconds: Number(el(key + "-seconds").value),
        };
      return config;
    }
    el("dialog").querySelector("input:invalid").reportValidity();
    throw Error("Check routine fields before saving.");
  }
  async function saveConfig() {
    config = await invoke("routines_config", { config: read() });
    el("message").textContent =
      "Saved. Changes reset the timer; scheduled profiles are snapshots of the selected saved scene.";
  }
  function scene(id) {
    const p = profiles.find((p) => p.id === el(id).value);
    if (!p)
      throw Error("Save an offline Color, Effects or Replay scene first.");
    return {
      name: p.name,
      settings: structuredClone(p.settings),
      source: structuredClone(
        p.source || { screenId: "", captureSource: { kind: "display" } },
      ),
    };
  }
  function init() {
    el("open").onclick = () =>
      task(async () => {
        config = await invoke("routines_config", { config: null });
        profiles = (
          await invoke("profile_library", { library: null })
        ).profiles.filter((p) =>
          ["solid", "rainbow", "replay"].includes(p.settings.mode),
        );
        for (const id of ["scene", "app-scene"]) {
          el(id).replaceChildren();
          for (const p of profiles) el(id).add(new Option(p.name, p.id));
        }
        draw();
        sync(runtime);
        el("message").textContent = "";
        el("dialog").showModal();
      });
    el("close").onclick = () => el("dialog").close();
    el("save").onclick = () => task(saveConfig);
    el("add-schedule").onclick = () =>
      task(async () => {
        const days = [...el("days").querySelectorAll("input:checked")].map(
          (i) => Number(i.value),
        );
        if (!days.length) throw Error("Choose at least one day.");
        if (config.schedules.length >= 32)
          throw Error("Keep up to 32 schedules.");
        config.schedules.push({
          id: crypto.randomUUID(),
          enabled: true,
          days,
          minute: minute(el("time").value),
          scene: scene("scene"),
          transition: el("transition").value,
          seconds: Number(el("seconds").value),
        });
        rows();
      });
    el("refresh-apps").onclick = () =>
      task(async () => {
        const names = await invoke("routine_apps");
        el("app").replaceChildren();
        for (const name of names) el("app").add(new Option(name, name));
        el("message").textContent = names.length
          ? "Choose an application. Only its app name is used; window titles and notification text are not stored."
          : "No applications available. Check OS permissions; Wayland app rules are unavailable.";
      });
    el("add-rule").onclick = () =>
      task(async () => {
        if (!el("app").value)
          throw Error("Refresh and select an application first.");
        if (config.appRules.length >= 32)
          throw Error("Keep up to 32 app rules.");
        config.appRules.push({
          app: el("app").value,
          scene: scene("app-scene"),
        });
        rows();
      });
    el("start").onclick = () =>
      task(async () => {
        await saveConfig();
        await request({ type: "routine", command: "start" });
      });
    for (const [id, command] of [
      ["pause-timer", "pause"],
      ["skip", "skip"],
      ["cancel", "cancel"],
      ["cue", "cue"],
      ["resume-rules", "resumeRules"],
    ])
      el(id).onclick = () => task(() => request({ type: "routine", command }));
  }
  return { init, sync };
})();
