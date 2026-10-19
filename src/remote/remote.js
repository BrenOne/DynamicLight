"use strict";
const el = (id) => document.getElementById(id);
let busy = false,
  paired = false,
  paused = false,
  previousStatus = "";
function status(message) {
  if (message !== previousStatus) {
    el("status").textContent = message;
    previousStatus = message;
  }
}
function disable(value) {
  el("controls")
    .querySelectorAll("button,input,select")
    .forEach((x) => (x.disabled = value));
}
async function post(path, body) {
  const response = await fetch(path, {
    method: "POST",
    credentials: "same-origin",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
    signal: AbortSignal.timeout(6000),
  });
  const result = await response.json();
  if (response.status === 401) {
    paired = false;
    el("pair").hidden = false;
    el("controls").hidden = true;
  }
  if (!response.ok || !result.ok) throw Error(result.error || "Request failed");
  return result.data;
}
async function command(value) {
  return post("/v1/command", { version: 1, command: value });
}
function render(s) {
  paired = true;
  paused = !!s.paused;
  el("pair").hidden = true;
  el("controls").hidden = false;
  if (document.activeElement !== el("brightness")) {
    el("brightness").value = s.brightness;
    el("level").textContent = s.brightness + "%";
  }
  el("state").textContent =
    `${s.powered ? "On" : s.requested ? "Waiting for strip" : "Off"} · ${s.mode}`;
  status(
    paused
      ? "Host lighting paused. Resume on the desktop."
      : s.connected
        ? "Connected to your strip"
        : "Desktop connected; strip unavailable or stopped",
  );
  disable(busy || paused);
}
async function profiles() {
  const list = await command({ command: "profiles" });
  const selected = el("profile").value;
  el("profile").replaceChildren();
  for (const p of list) {
    const option = new Option(p.name, p.id);
    if (["screen", "music", "blend"].includes(p.mode)) {
      option.disabled = true;
      option.textContent += " · desktop capture required";
    }
    el("profile").add(option);
  }
  if (
    [...el("profile").options].some((o) => o.value === selected && !o.disabled)
  )
    el("profile").value = selected;
  else
    el("profile").selectedIndex = [...el("profile").options].findIndex(
      (o) => !o.disabled,
    );
}
async function task(fn) {
  if (busy) return;
  busy = true;
  disable(true);
  el("pair").querySelector("button").disabled = true;
  let success = false;
  try {
    await fn();
    success = true;
  } catch (e) {
    status(
      e instanceof TypeError || e.name === "TimeoutError"
        ? "Disconnected or host asleep. Reconnect when it is available."
        : String(e.message || e),
    );
    disable(true);
  } finally {
    busy = false;
    if (success && paired) {
      disable(paused);
      el("off").disabled = false;
    }
    el("pair").querySelector("button").disabled = false;
  }
}
async function refresh() {
  try {
    const s = await command({ command: "status" });
    const first = !paired;
    render(s);
    if (first) await profiles();
  } catch (e) {
    disable(true);
    throw e;
  }
}
el("pair").onsubmit = (e) => {
  e.preventDefault();
  task(async () => {
    await post("/pair", { code: el("code").value, name: el("name").value });
    el("code").value = "";
    await refresh();
  });
};
for (const state of ["on", "off"])
  el(state).onclick = () =>
    task(async () => {
      render(await command({ command: "power", state }));
    });
el("brightness").oninput = () =>
  (el("level").textContent = el("brightness").value + "%");
el("brightness").onchange = () =>
  task(async () => {
    render(
      await command({
        command: "brightness",
        value: Number(el("brightness").value),
      }),
    );
  });
el("set-color").onclick = () =>
  task(async () => {
    render(await command({ command: "color", value: el("color").value }));
  });
el("apply").onclick = () =>
  task(async () => {
    if (!el("profile").value)
      throw Error("Create an offline profile in the desktop app first.");
    render(await command({ command: "applyProfile", id: el("profile").value }));
  });
el("retry").onclick = () =>
  task(async () => {
    await refresh();
    if (paired) await profiles();
  });
setInterval(() => {
  if (paired && !busy && !document.hidden) task(refresh);
}, 5000);
task(async () => {
  await refresh();
});
