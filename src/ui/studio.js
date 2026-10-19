"use strict";
window.Studio = (() => {
  const el = (id) => document.getElementById("studio-" + id);
  const clone = (value) => structuredClone(value);
  const defaultEffect = () => ({
    version: 1,
    durationMs: 6000,
    repeats: 0,
    layers: [
      {
        kind: "wave",
        opacity: 100,
        stops: [
          { position: 0, color: "#7046ff" },
          { position: 1000, color: "#20dbaa" },
        ],
      },
    ],
  });
  const defaultStudio = () => ({
    effect: defaultEffect(),
    zones: [],
    bias: {
      enabled: false,
      kelvin: 6500,
      gains: [100, 100, 100],
      measurement: "",
    },
  });
  let draft,
    presets,
    section,
    selected = new Set(),
    anchor = 0,
    history = [],
    timer,
    previewBusy = false,
    generation = 0,
    started,
    points = [];
  const message = (value) => {
    el("status").textContent = value;
  };
  function remember() {
    history.push(clone(draft));
    if (history.length > 40) history.shift();
    el("undo").disabled = false;
  }
  function mutate(fn, redraw = true) {
    remember();
    fn();
    if (redraw) render();
    preview();
  }
  function number(value, min, max) {
    const n = Number(value);
    if (value === "" || !Number.isFinite(n) || n < min || n > max)
      throw Error(`Use a number from ${min} to ${max}.`);
    return n;
  }
  function label(text, input) {
    const l = document.createElement("label");
    l.append(document.createTextNode(text), input);
    return l;
  }
  function input(type, value, onchange, min, max) {
    const n = document.createElement("input");
    n.type = type;
    n.value = value;
    if (min !== undefined) n.min = min;
    if (max !== undefined) n.max = max;
    n.onchange = () => {
      if (n.reportValidity())
        mutate(() => onchange(type === "number" ? Number(n.value) : n.value));
    };
    return n;
  }
  function button(text, action) {
    const b = document.createElement("button");
    b.type = "button";
    b.textContent = text;
    b.onclick = action;
    return b;
  }
  function select(options, value, onchange) {
    const s = document.createElement("select");
    s.replaceChildren(...options.map(([v, l]) => new Option(l, v)));
    s.value = value;
    s.onchange = () => mutate(() => onchange(s.value));
    return s;
  }
  function renderLayers() {
    el("layers").replaceChildren(
      ...draft.studio.effect.layers.map((layer, index) => {
        const group = document.createElement("fieldset"),
          legend = document.createElement("legend");
        legend.textContent = `Layer ${index + 1}`;
        group.append(legend);
        group.append(
          label(
            "Building block",
            select(
              [
                ["gradient", "Gradient"],
                ["wave", "Moving wave"],
                ["breath", "Breathing gradient"],
                ["sequence", "Color keyframes"],
              ],
              layer.kind,
              (v) => (layer.kind = v),
            ),
          ),
          label(
            "Opacity %",
            input("number", layer.opacity, (v) => (layer.opacity = v), 0, 100),
          ),
        );
        const stops = document.createElement("div");
        stops.className = "studio-stops";
        layer.stops.forEach((stop, i) => {
          const row = document.createElement("div");
          row.className = "input-row";
          const pos = input(
            "number",
            stop.position,
            (v) => {
              stop.position = v;
              layer.stops.sort((a, b) => a.position - b.position);
            },
            0,
            1000,
          );
          pos.disabled = i === 0 || i === layer.stops.length - 1;
          row.append(
            label(`Stop ${i + 1} position /1000`, pos),
            label(
              `Stop ${i + 1} color`,
              input("color", stop.color, (v) => (stop.color = v)),
            ),
          );
          if (i > 0 && i < layer.stops.length - 1)
            row.append(
              button("Remove stop", () =>
                mutate(() => layer.stops.splice(i, 1)),
              ),
            );
          stops.append(row);
        });
        group.append(stops);
        if (layer.stops.length < 16)
          group.append(
            button("Add color stop", () =>
              mutate(() => {
                let at = 1,
                  gap = 0;
                for (let i = 1; i < layer.stops.length; i++) {
                  const d =
                    layer.stops[i].position - layer.stops[i - 1].position;
                  if (d > gap) {
                    gap = d;
                    at = i;
                  }
                }
                if (gap > 1)
                  layer.stops.splice(at, 0, {
                    position: Math.floor(
                      (layer.stops[at].position +
                        layer.stops[at - 1].position) /
                        2,
                    ),
                    color: layer.stops[at - 1].color,
                  });
              }),
            ),
          );
        if (index > 0)
          group.append(
            button("Move layer up", () =>
              mutate(() => {
                const layers = draft.studio.effect.layers;
                [layers[index - 1], layers[index]] = [
                  layers[index],
                  layers[index - 1],
                ];
              }),
            ),
          );
        if (draft.studio.effect.layers.length > 1)
          group.append(
            button("Remove layer", () =>
              mutate(() => draft.studio.effect.layers.splice(index, 1)),
            ),
          );
        return group;
      }),
    );
    el("add-layer").disabled = draft.studio.effect.layers.length >= 4;
  }
  function renderZones() {
    el("zones").replaceChildren(
      ...draft.studio.zones.map((zone, index) => {
        const group = document.createElement("fieldset"),
          legend = document.createElement("legend");
        legend.textContent = `Zone ${index + 1}`;
        group.append(legend);
        const name = input("text", zone.name, (v) => (zone.name = v));
        name.maxLength = 50;
        group.append(
          label("Name", name),
          label(
            "First LED",
            input(
              "number",
              Math.floor((zone.start / 1000) * draft.count) + 1,
              (v) => (zone.start = Math.round(((v - 1) / draft.count) * 1000)),
              1,
              draft.count,
            ),
          ),
          label(
            "Last LED",
            input(
              "number",
              Math.round((zone.end / 1000) * draft.count),
              (v) => (zone.end = Math.round((v / draft.count) * 1000)),
              1,
              draft.count,
            ),
          ),
          label(
            "Color",
            input("color", zone.color, (v) => (zone.color = v)),
          ),
        );
        const choices = [
          ["", "Static color"],
          ["current", "Current custom effect"],
          ...presets.effects.map((p, i) => [String(i), p.name]),
        ];
        group.append(
          label(
            "Behavior",
            select(
              choices,
              zone.effect ? "current" : "",
              (v) =>
                (zone.effect =
                  v === ""
                    ? null
                    : clone(
                        v === "current"
                          ? draft.studio.effect
                          : presets.effects[Number(v)].effect,
                      )),
            ),
          ),
        );
        if (index > 0)
          group.append(
            button("Move zone up", () =>
              mutate(() => {
                const z = draft.studio.zones;
                [z[index - 1], z[index]] = [z[index], z[index - 1]];
              }),
            ),
          );
        group.append(
          button("Remove zone", () =>
            mutate(() => draft.studio.zones.splice(index, 1)),
          ),
        );
        return group;
      }),
    );
  }
  function selection() {
    el("selection").textContent = selected.size
      ? `${selected.size} selected · LEDs ${[...selected]
          .sort((a, b) => a - b)
          .map((i) => i + 1)
          .join(", ")}`
      : "Select LEDs or an edge. Shift extends the selection.";
    [...el("leds").children].forEach((b, i) =>
      b.setAttribute("aria-pressed", String(selected.has(i))),
    );
  }
  function choose(i, extend) {
    if (extend) {
      selected = new Set(
        Array.from(
          { length: Math.abs(i - anchor) + 1 },
          (_, n) => Math.min(i, anchor) + n,
        ),
      );
    } else {
      anchor = i;
      selected = new Set([i]);
    }
    selection();
  }
  function positions() {
    const aspect = 16 / 9,
      perimeter = 2 + aspect * (draft.sides === 4 ? 2 : 1);
    return Array.from({ length: draft.count }, (_, i) => {
      const p = ((i + 0.5) / draft.count) * perimeter;
      return p < 1
        ? [0, 1 - p]
        : p < 1 + aspect
          ? [(p - 1) / aspect, 0]
          : p < 2 + aspect
            ? [1, p - 1 - aspect]
            : [1 - (p - 2 - aspect) / aspect, 1];
    });
  }
  function render() {
    for (const name of ["effect", "zones", "bias"])
      el(name + "-panel").hidden = section !== name;
    el("preset-controls").hidden = section === "zones";
    el("title").textContent = {
      effect: "Custom effects",
      zones: "LED layout and zones",
      bias: "Bias-light calibration",
    }[section];
    el("duration").value = draft.studio.effect.durationMs / 1000;
    el("repeats").value = draft.studio.effect.repeats;
    el("speed").value = draft.speed;
    el("reverse").checked = draft.effectReverse;
    el("kelvin").value = draft.studio.bias.kelvin;
    el("brightness").value = draft.brightness;
    ["red", "green", "blue"].forEach(
      (c, i) => (el(c).value = draft.studio.bias.gains[i]),
    );
    el("measurement").value = draft.studio.bias.measurement;
    el("offset").value = draft.offset;
    el("route-reverse").checked = draft.reverse;
    el("sides").value = draft.sides;
    el("effect-presets").replaceChildren(
      new Option("Choose an effect…", ""),
      ...presets.effects.map((p, i) => new Option(p.name, String(i))),
    );
    el("bias-presets").replaceChildren(
      new Option("Choose a calibration…", ""),
      ...presets.bias.map((p, i) => new Option(p.name, String(i))),
    );
    renderLayers();
    renderZones();
    el("leds").replaceChildren(
      ...Array.from({ length: draft.count }, (_, i) => {
        const b = button(String(i + 1), () => {});
        b.setAttribute("aria-label", `LED ${i + 1}`);
        b.onclick = (e) => choose(i, e.shiftKey);
        b.onkeydown = (e) => {
          if (["ArrowLeft", "ArrowRight", "Home", "End"].includes(e.key)) {
            e.preventDefault();
            const next =
              e.key === "Home"
                ? 0
                : e.key === "End"
                  ? draft.count - 1
                  : Math.max(
                      0,
                      Math.min(
                        draft.count - 1,
                        i + (e.key === "ArrowRight" ? 1 : -1),
                      ),
                    );
            choose(next, e.shiftKey);
            el("leds").children[next].focus();
          }
        };
        return b;
      }),
    );
    selection();
    points = positions();
  }
  async function preview() {
    clearTimeout(timer);
    if (previewBusy || !el("dialog").open || document.hidden) return;
    previewBusy = true;
    const ticket = generation;
    try {
      const seconds = el("animate").checked
        ? (performance.now() - started) / 1000
        : 2;
      const colors = await invoke("studio_preview", {
        settings: clone(draft),
        seconds: seconds % 86400,
      });
      if (ticket !== generation) return;
      const canvas = el("preview"),
        ctx = canvas.getContext("2d");
      ctx.clearRect(0, 0, canvas.width, canvas.height);
      ctx.fillStyle = getComputedStyle(
        document.documentElement,
      ).getPropertyValue("--stage");
      ctx.fillRect(0, 0, canvas.width, canvas.height);
      ctx.strokeStyle = getComputedStyle(
        document.documentElement,
      ).getPropertyValue("--muted");
      ctx.strokeRect(30, 25, 540, 210);
      points.forEach(([x, y], i) => {
        ctx.fillStyle = `rgb(${colors[i].join(",")})`;
        ctx.fillRect(24 + x * 540, 19 + y * 210, 12, 12);
        if (selected.has(i)) {
          ctx.strokeStyle = getComputedStyle(
            document.documentElement,
          ).getPropertyValue("--text");
          ctx.strokeRect(21 + x * 540, 16 + y * 210, 18, 18);
        }
      });
      if (section === "zones" && points.length) {
        const first = draft.reverse
          ? draft.count - 1 - (draft.offset % draft.count)
          : draft.offset % draft.count;
        const [x, y] = points[first];
        ctx.fillStyle = getComputedStyle(
          document.documentElement,
        ).getPropertyValue("--text");
        ctx.font = "bold 16px sans-serif";
        ctx.fillText("①", 18 + x * 540, 16 + y * 210);
      }
    } catch (error) {
      message(String(error));
    } finally {
      previewBusy = false;
      if (el("dialog").open)
        timer = setTimeout(preview, el("animate").checked ? 100 : 500);
    }
  }
  async function open(kind) {
    try {
      await save();
      presets = await invoke("studio_library", { next: null, expected: null });
      draft = clone(settings);
      draft.studio = clone(settings.studio || defaultStudio());
      section = kind;
      history = [];
      selected = new Set();
      el("undo").disabled = true;
      el("preset-name").value = "";
      message("");
      if (kind === "effect") {
        draft.mode = "rainbow";
        draft.effect = "custom";
      }
      if (kind === "bias") {
        draft.mode = "solid";
        draft.studio.bias.enabled = true;
        draft.studio.zones = [];
      }
      started = performance.now();
      generation++;
      render();
      el("dialog").showModal();
      el("title").focus();
      preview();
    } catch (error) {
      notice(String(error), true);
    }
  }
  async function savePreset() {
    const name = el("preset-name").value.trim();
    if (!name) throw Error("Enter a preset name.");
    const next = clone(presets),
      items = section === "bias" ? next.bias : next.effects;
    const value =
      section === "bias"
        ? { name, bias: clone(draft.studio.bias), brightness: draft.brightness }
        : { name, effect: clone(draft.studio.effect) };
    const at = items.findIndex((p) => p.name === name);
    if (at < 0) items.push(value);
    else items[at] = value;
    presets = await invoke("studio_library", { next, expected: presets });
    render();
    message("Preset saved. Lighting is unchanged until Apply.");
  }
  async function task(fn) {
    try {
      await fn();
    } catch (error) {
      message(String(error));
    }
  }
  function init() {
    for (const kind of ["effect", "zones", "bias"])
      document.getElementById("open-studio-" + kind).onclick = () => open(kind);
    el("animate").checked = !matchMedia("(prefers-reduced-motion: reduce)")
      .matches;
    el("dialog").addEventListener("close", () => {
      generation++;
      clearTimeout(timer);
    });
    el("cancel").onclick = () => el("dialog").close();
    el("undo").onclick = () => {
      if (history.length) {
        draft = history.pop();
        render();
        preview();
      }
      el("undo").disabled = !history.length;
    };
    el("apply").onclick = () =>
      task(async () => {
        if (
          ![...el("dialog").querySelectorAll("input")].every((n) =>
            n.reportValidity(),
          )
        )
          return;
        await invoke("studio_preview", { settings: clone(draft), seconds: 0 });
        const patch = { studio: draft.studio };
        if (section === "effect")
          Object.assign(patch, {
            mode: "rainbow",
            effect: "custom",
            speed: draft.speed,
            effectReverse: draft.effectReverse,
          });
        if (section === "bias")
          Object.assign(patch, { mode: "solid", brightness: draft.brightness });
        if (section === "zones")
          Object.assign(patch, {
            offset: draft.offset,
            reverse: draft.reverse,
            sides: draft.sides,
          });
        const value = await request({
          type: "patchSettings",
          patch,
          persist: true,
        });
        acceptSettings(value);
        updateUI();
        el("dialog").close();
        notice("Studio changes applied. Use Start if lighting is off.");
      });
    const numeric = (id, min, max, set) =>
      (el(id).onchange = () =>
        task(() => {
          const value = number(el(id).value, min, max);
          mutate(() => set(value));
        }));
    numeric(
      "duration",
      0.1,
      3600,
      (v) => (draft.studio.effect.durationMs = Math.round(v * 1000)),
    );
    numeric("repeats", 0, 10000, (v) => (draft.studio.effect.repeats = v));
    numeric("speed", 1, 100, (v) => (draft.speed = v));
    numeric("kelvin", 2700, 10000, (v) => (draft.studio.bias.kelvin = v));
    numeric("brightness", 0, 100, (v) => (draft.brightness = v));
    ["red", "green", "blue"].forEach((c, i) =>
      numeric(c, 0, 200, (v) => (draft.studio.bias.gains[i] = v)),
    );
    numeric("offset", 0, 126, (v) => (draft.offset = v));
    el("reverse").onchange = (e) =>
      mutate(() => (draft.effectReverse = e.target.checked));
    el("route-reverse").onchange = (e) =>
      mutate(() => (draft.reverse = e.target.checked));
    el("sides").onchange = (e) =>
      mutate(() => (draft.sides = Number(e.target.value)));
    el("measurement").onchange = (e) =>
      mutate(() => (draft.studio.bias.measurement = e.target.value));
    el("add-layer").onclick = () =>
      mutate(() => {
        if (draft.studio.effect.layers.length < 4)
          draft.studio.effect.layers.push(defaultEffect().layers[0]);
      });
    el("save-preset").onclick = () => task(savePreset);
    for (const kind of ["effect", "bias"])
      el(kind + "-presets").onchange = (e) => {
        if (e.target.value === "") return;
        const p =
          presets[kind === "effect" ? "effects" : "bias"][
            Number(e.target.value)
          ];
        mutate(() => {
          draft.studio[kind] = clone(p[kind]);
          if (kind === "bias") {
            draft.brightness = p.brightness;
            draft.studio.bias.enabled = true;
          }
        });
        el("preset-name").value = p.name;
      };
    el("delete-preset").onclick = () =>
      task(async () => {
        const key = section === "bias" ? "bias" : "effects",
          name = el("preset-name").value;
        const next = clone(presets);
        next[key] = next[key].filter((p) => p.name !== name);
        presets = await invoke("studio_library", { next, expected: presets });
        render();
        message("Matching saved preset removed; draft kept.");
      });
    el("disable-bias").onclick = () =>
      mutate(() => {
        draft.studio.bias.enabled = false;
      });
    el("add-zone").onclick = () =>
      task(() => {
        if (!selected.size) throw Error("Select LEDs first.");
        if (draft.studio.zones.length >= 16)
          throw Error("Use at most 16 zones.");
        mutate(() =>
          draft.studio.zones.push({
            name: `Zone ${draft.studio.zones.length + 1}`,
            start: Math.round((Math.min(...selected) / draft.count) * 1000),
            end: Math.round(((Math.max(...selected) + 1) / draft.count) * 1000),
            color: el("zone-color").value,
            effect: null,
          }),
        );
      });
    el("clear-zones").onclick = () => mutate(() => (draft.studio.zones = []));
    for (const edge of ["left", "top", "right", "bottom", "all"])
      el("edge-" + edge).onclick = () => {
        selected = new Set(
          points.flatMap(([x, y], i) =>
            edge === "all" ||
            (edge === "left" && x === 0) ||
            (edge === "right" && x === 1) ||
            (edge === "top" && y === 0) ||
            (edge === "bottom" && y === 1)
              ? [i]
              : [],
          ),
        );
        selection();
      };
    el("preview").onclick = (e) => {
      const r = e.target.getBoundingClientRect(),
        x = ((e.clientX - r.left) / r.width) * 600,
        y = ((e.clientY - r.top) / r.height) * 260;
      let nearest = 0,
        distance = Infinity;
      points.forEach(([px, py], i) => {
        const d = Math.hypot(x - 30 - px * 540, y - 25 - py * 210);
        if (d < distance) {
          distance = d;
          nearest = i;
        }
      });
      if (distance < 30) choose(nearest, e.shiftKey);
    };
    el("export").onclick = () =>
      task(async () => {
        const value = {
          format: "dynamiclight-effect",
          version: 1,
          effect: draft.studio.effect,
        };
        const blob = new Blob([JSON.stringify(value, null, 2)], {
            type: "application/json",
          }),
          a = document.createElement("a"),
          url = URL.createObjectURL(blob);
        a.href = url;
        a.download = "dynamiclight-effect.json";
        a.click();
        setTimeout(() => URL.revokeObjectURL(url), 1000);
      });
    el("import").onchange = (e) =>
      task(async () => {
        const file = e.target.files[0];
        if (!file) return;
        if (file.size > 32768)
          throw Error("Effect files must be at most 32 KiB.");
        const data = JSON.parse(await file.text());
        if (data.format !== "dynamiclight-effect" || data.version !== 1)
          throw Error("Unsupported effect format.");
        const candidate = clone(draft);
        candidate.studio.effect = data.effect;
        await invoke("studio_preview", { settings: candidate, seconds: 0 });
        mutate(() => (draft.studio.effect = data.effect));
        e.target.value = "";
      });
  }
  return { init };
})();
