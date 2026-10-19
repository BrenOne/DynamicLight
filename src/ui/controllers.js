"use strict";
window.ControllerRouting = (() => {
  let handoffText = "";
  const el = (id) => document.getElementById("controller-" + id);
  let fleet,
    screens = [],
    devices = [],
    routes = [],
    positions = {},
    drag,
    boxes = [],
    working = false;
  const clone = (value) => structuredClone(value);
  const rowStatus = (row) =>
    row?.state
      ? `${row.state.status.message} · ${row.state.requested ? "Running" : "Stopped"} · ${row.state.settings.brightness}% individual level · ${row.state.groupBrightness ?? 100}% group scale${row.state.error ? " · " + row.state.error : ""}`
      : "Worker starting";
  function sync(value) {
    const name =
      fleet?.controllers.find((r) => r.key === fleet.selected)?.name ||
      "Main strip";
    el("selected-label").textContent =
      name +
      (value?.handoffReleased
        ? " · released"
        : value?.groupBrightness < 100
          ? ` · group ${value.groupBrightness}%`
          : "");
  }
  async function task(fn) {
    if (working) return;
    working = true;
    el("dialog")
      .querySelectorAll("button:not(#controller-close)")
      .forEach((b) => (b.disabled = true));
    try {
      await fn();
    } catch (e) {
      el("message").textContent = String(e);
    } finally {
      working = false;
      el("dialog")
        .querySelectorAll("button")
        .forEach((b) => (b.disabled = false));
    }
  }
  function selectOptions(select, items, value) {
    select.replaceChildren();
    for (const [id, name] of items) select.add(new Option(name, id));
    select.value = value;
  }
  function loadDraft() {
    positions = Object.fromEntries(
      screens
        .slice(0, 16)
        .map((s) => [
          s.id,
          clone(fleet.positions?.[s.id] || { x: s.x || 0, y: s.y || 0 }),
        ]),
    );
    screens = screens.slice(0, 16);
    routes = fleet.controllers
      .filter((r) => r.state)
      .map((r) => ({
        key: r.key,
        screenId: r.state.settings.screenId,
        count: r.state.settings.count,
        sides: r.state.settings.sides,
        offset: r.state.settings.offset,
        reverse: r.state.settings.reverse,
      }));
    selectOptions(
      el("selected"),
      fleet.controllers.map((r) => [r.key, r.name]),
      fleet.selected,
    );
    el("selected-label").textContent =
      fleet.controllers.find((r) => r.key === fleet.selected)?.name ||
      "Main strip";
    el("gain").value =
      fleet.controllers.find((r) => r.key === fleet.selected)?.state
        ?.groupBrightness ?? 100;
    selected();
    monitorFields();
    draw();
  }
  function selected() {
    const route = routes.find((r) => r.key === el("selected").value),
      row = fleet.controllers.find((r) => r.key === el("selected").value);
    selectOptions(
      el("display"),
      [["", "Primary display"], ...screens.map((s) => [s.id, s.name])],
      route?.screenId || "",
    );
    if (route?.screenId && !screens.some((s) => s.id === route.screenId)) {
      el("display").add(
        new Option("Unavailable display · " + route.screenId, route.screenId),
      );
      el("display").value = route.screenId;
    }
    for (const key of ["count", "sides", "offset"])
      el(key).value = route?.[key] ?? { count: 65, sides: 3, offset: 0 }[key];
    el("reverse").checked = !!route?.reverse;
    el("state").textContent = row?.state
      ? `${row.state.status.message} · ${row.state.requested ? "Running" : "Stopped"} · ${row.state.settings.brightness}% individual level · ${row.state.groupBrightness ?? 100}% group scale${row.state.error ? " · " + row.state.error : ""}`
      : "Worker starting";
    draw();
  }
  function editRoute() {
    const route = routes.find((r) => r.key === el("selected").value);
    if (!route) return;
    Object.assign(route, {
      screenId: el("display").value,
      count: Number(el("count").value),
      sides: Number(el("sides").value),
      offset: Number(el("offset").value),
      reverse: el("reverse").checked,
    });
    draw();
  }
  function monitorFields() {
    selectOptions(
      el("monitor"),
      screens.map((s) => [s.id, s.name]),
      el("monitor").value || screens[0]?.id || "",
    );
    const p = positions[el("monitor").value] || { x: 0, y: 0 };
    el("x").value = p.x;
    el("y").value = p.y;
  }
  function draw() {
    const canvas = el("canvas"),
      ctx = canvas.getContext("2d"),
      theme = window.LightUI?.theme?.() || {};
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    ctx.fillStyle = theme.text || "#a7a7bb";
    ctx.font = "14px sans-serif";
    boxes = [];
    if (!screens.length) {
      ctx.fillText(
        "Display arrangement unavailable; choose mappings using the form.",
        20,
        35,
      );
      return;
    }
    const minX = Math.min(...screens.map((s) => positions[s.id]?.x || 0)),
      minY = Math.min(...screens.map((s) => positions[s.id]?.y || 0));
    const maxX = Math.max(
        ...screens.map((s) => (positions[s.id]?.x || 0) + (s.width || 1920)),
      ),
      maxY = Math.max(
        ...screens.map((s) => (positions[s.id]?.y || 0) + (s.height || 1080)),
      );
    const scale = Math.min(
      (canvas.width - 80) / (maxX - minX),
      (canvas.height - 80) / (maxY - minY),
    );
    screens.forEach((s, i) => {
      const p = positions[s.id],
        x = 40 + (p.x - minX) * scale,
        y = 40 + (p.y - minY) * scale,
        w = (s.width || 1920) * scale,
        h = (s.height || 1080) * scale;
      boxes.push({ id: s.id, x, y, w, h, scale });
      ctx.fillStyle = "#202238";
      ctx.fillRect(x, y, w, h);
      ctx.strokeStyle = "#8b91b7";
      ctx.lineWidth = 2;
      ctx.strokeRect(x, y, w, h);
      ctx.fillStyle = "#f0f1ff";
      ctx.fillText(`Display ${i + 1}`, x + 8, y + 20);
      const assigned = routes.filter(
        (r) =>
          r.screenId === s.id ||
          (!r.screenId &&
            (s.primary ||
              (!screens.some((display) => display.primary) && i === 0))),
      );
      assigned.forEach((r, j) => {
        const color = ["#b794ff", "#62d6ab", "#ffbf6b", "#74b8ff"][j % 4],
          pad = 6 + j * 5;
        const routeW = Math.max(1, w - 2 * pad),
          routeH = Math.max(1, h - 2 * pad),
          length = routeH * 2 + routeW * (r.sides === 4 ? 2 : 1);
        function point(t) {
          let d = t * length;
          if (d < routeH) return [x + pad, y + h - pad - d];
          d -= routeH;
          if (d < routeW) return [x + pad + d, y + pad];
          d -= routeW;
          if (d < routeH) return [x + w - pad, y + pad + d];
          d -= routeH;
          return [x + w - pad - d, y + h - pad];
        }
        ctx.fillStyle = color;
        for (let n = 0; n < Math.min(127, Math.max(1, r.count)); n++) {
          const [px, py] = point(n / r.count);
          ctx.beginPath();
          ctx.arc(px, py, 2, 0, Math.PI * 2);
          ctx.fill();
        }
        const first = r.reverse
            ? r.count - 1 - (r.offset % Math.max(1, r.count))
            : r.offset % Math.max(1, r.count),
          [fx, fy] = point(first / Math.max(1, r.count));
        ctx.beginPath();
        ctx.arc(fx, fy, 5, 0, Math.PI * 2);
        ctx.fill();
        const name =
          fleet.controllers.find((c) => c.key === r.key)?.name || r.key;
        ctx.fillText(`${name} · ${r.count} LEDs`, x + 8, y + 40 + j * 18);
      });
    });
  }
  async function refresh() {
    fleet = await invoke("controller_list");
    try {
      screens = await invoke("displays");
    } catch (e) {
      screens = [];
      el("message").textContent = String(e);
    }
    devices = await invoke("controller_devices");
    selectOptions(
      el("usb"),
      devices.map((d) => [d.id, d.name]),
      devices[0]?.id || "",
    );
    loadDraft();
    await window.Ambient.load();
  }
  async function manage(operation, key, name = "") {
    await save();
    fleet = await invoke("controller_manage", { operation, key, name });
    const value = await request({ type: "snapshot" });
    acceptSettings(value);
    updateUI();
    loadDraft();
    await window.Ambient.load();
  }
  function init() {
    setInterval(async () => {
      if (!el("dialog").open || working) return;
      try {
        const value = await invoke("controller_list");
        if (working || !el("dialog").open) return;
        fleet = value;
        el("state").textContent = rowStatus(
          fleet.controllers.find((r) => r.key === el("selected").value),
        );
        for (const option of el("selected").options) {
          const row = fleet.controllers.find((r) => r.key === option.value);
          if (row)
            option.textContent =
              row.name +
              " · " +
              (row.state?.status.connected
                ? "connected"
                : row.state?.requested
                  ? "recovering"
                  : "stopped");
        }
      } catch {}
    }, 2000);
    el("handoff-export").onclick = () =>
      task(async () => {
        await save();
        const path = await invoke("handoff_export");
        await refresh();
        el("message").textContent =
          "Released controlled strip. Handoff saved to " +
          path +
          ". Switch the physical USB connection to the other computer.";
      });
    el("handoff-file").onchange = () =>
      task(async () => {
        handoffText = "";
        el("handoff-preview").textContent = "";
        const file = el("handoff-file").files[0];
        if (!file) return;
        if (file.size > 256 * 1024)
          throw Error("Handoff files must be at most 256 KiB.");
        const text = await file.text(),
          profile = await invoke("handoff_preview", { text });
        handoffText = text;
        el("handoff-preview").textContent =
          `Compatible ROBOBLOQ controller · ${profile.count} LEDs · ${profile.sides} sides · ${profile.scene.mode} · ${profile.scene.brightness}% · offset ${profile.offset}${profile.reverse ? " · reversed" : ""}. Apply replaces the controlled strip's scene and calibration, without starting it.`;
      });
    el("handoff-import").onclick = () =>
      task(async () => {
        if (!handoffText)
          throw Error("Choose and preview a handoff file first.");
        await save();
        const value = await invoke("handoff_import", { text: handoffText });
        acceptSettings(value);
        renderRuntime(value);
        updateUI();
        await refresh();
        el("message").textContent =
          "Profile and calibration restored; lighting is stopped. Bind USB/display on this computer and press Start when ready.";
      });
    invoke("controller_list")
      .then((value) => {
        if (value?.controllers) {
          fleet = value;
          sync(runtime);
        }
      })
      .catch(() => {});
    el("open").onclick = () =>
      task(async () => {
        await save();
        await refresh();
        el("dialog").showModal();
      });
    el("close").onclick = () => el("dialog").close();
    el("refresh").onclick = () => task(refresh);
    el("selected").onchange = selected;
    el("control").onclick = () =>
      task(() => manage("select", el("selected").value));
    el("bind").onclick = () =>
      task(async () => {
        await manage("select", el("selected").value);
        await manage("bind", el("usb").value);
      });
    el("add").onclick = () =>
      task(() => manage("add", el("usb").value, el("name").value));
    el("remove").onclick = () =>
      task(() => manage("remove", el("selected").value));
    el("studio").onclick = () =>
      task(async () => {
        await manage("select", el("selected").value);
        el("dialog").close();
        document.getElementById("open-studio-zones").click();
      });
    for (const key of ["display", "count", "sides", "offset", "reverse"])
      el(key).oninput = editRoute;
    el("monitor").onchange = monitorFields;
    for (const key of ["x", "y"])
      el(key).oninput = () => {
        const id = el("monitor").value;
        if (id) {
          positions[id] = {
            x: Number(el("x").value),
            y: Number(el("y").value),
          };
          draw();
        }
      };
    el("apply").onclick = () =>
      task(async () => {
        const invalid = el("dialog").querySelector("input:invalid");
        if (invalid) {
          invalid.reportValidity();
          return;
        }
        await save();
        fleet = await invoke("controller_routes", { routes, positions });
        loadDraft();
        el("message").textContent =
          "Mappings applied. Changed capture sources require Start / Retry. Diagram positions do not rearrange OS displays.";
      });
    for (const [id, operation] of [
      ["start-all", "start"],
      ["stop-all", "stop"],
      ["gain-apply", "brightness"],
    ])
      el(id).onclick = () =>
        task(async () => {
          await save();
          fleet = await invoke("controller_group", {
            operation,
            value: Number(el("gain").value),
          });
          selected();
        });
    const canvas = el("canvas");
    const point = (e) => {
      const rect = canvas.getBoundingClientRect();
      return {
        x: ((e.clientX - rect.left) * canvas.width) / rect.width,
        y: ((e.clientY - rect.top) * canvas.height) / rect.height,
      };
    };
    canvas.onpointerdown = (e) => {
      const p = point(e),
        b = [...boxes]
          .reverse()
          .find(
            (b) =>
              p.x >= b.x && p.x <= b.x + b.w && p.y >= b.y && p.y <= b.y + b.h,
          );
      if (b) {
        drag = { ...b, start: p, origin: clone(positions[b.id]) };
        el("monitor").value = b.id;
        monitorFields();
        canvas.setPointerCapture(e.pointerId);
      }
    };
    canvas.onpointermove = (e) => {
      if (!drag) return;
      const p = point(e);
      positions[drag.id] = {
        x: Math.round(
          Math.max(
            -100000,
            Math.min(100000, drag.origin.x + (p.x - drag.start.x) / drag.scale),
          ),
        ),
        y: Math.round(
          Math.max(
            -100000,
            Math.min(100000, drag.origin.y + (p.y - drag.start.y) / drag.scale),
          ),
        ),
      };
      monitorFields();
      draw();
    };
    canvas.onpointerup = canvas.onpointercancel = () => {
      drag = null;
    };
    canvas.onkeydown = (e) => {
      const delta = {
          ArrowLeft: [-50, 0],
          ArrowRight: [50, 0],
          ArrowUp: [0, -50],
          ArrowDown: [0, 50],
        }[e.key],
        id = el("monitor").value;
      if (delta && positions[id]) {
        e.preventDefault();
        positions[id].x = Math.max(
          -100000,
          Math.min(100000, positions[id].x + delta[0]),
        );
        positions[id].y = Math.max(
          -100000,
          Math.min(100000, positions[id].y + delta[1]),
        );
        monitorFields();
        draw();
      }
    };
  }
  return { init, sync };
})();
