"use strict";
window.Extensions = (() => {
  const el = (id) => document.getElementById("extension-" + id);
  let working = false,
    pluginText = "",
    plugins = [];
  const events = [
    ["damage", "Damage", "#ff5544"],
    ["kill", "Kill", "#55cc99"],
    ["roundStart", "Round starts", "#5588ff"],
    ["bombPlanted", "Bomb planted", "#ffbb44"],
  ];
  async function task(fn) {
    if (working) return;
    working = true;
    el("dialog")
      .querySelectorAll("button:not(#extension-close)")
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
      el("install").disabled = !pluginText;
    }
  }
  function gameStatus(s) {
    el("game-status").textContent = !s.enabled
      ? "Telemetry disabled"
      : `${s.connected ? "Receiving your active player state" : "Waiting for Counter-Strike 2"} · ${s.endpoint}`;
  }
  async function refresh() {
    const fleet = await invoke("controller_list");
    const selected = el("controller").value || fleet.selected;
    el("controller").replaceChildren(
      ...fleet.controllers.map((c) => new Option(c.name, c.key)),
    );
    el("controller").value = fleet.controllers.some((c) => c.key === selected)
      ? selected
      : fleet.selected;
    gameStatus(await invoke("game_status"));
    plugins = await invoke("plugin_manage", { operation: "list" });
    renderPlugins();
  }
  function renderPlugins() {
    el("plugins").replaceChildren();
    if (!plugins.length) el("plugins").textContent = "No plugins installed.";
    for (const entry of plugins) {
      const row = document.createElement("fieldset"),
        title = document.createElement("legend");
      const m = entry.manifest;
      title.textContent = `${m.name} ${m.version} · ${entry.enabled ? "Enabled" : "Disabled"}`;
      row.append(title);
      const details = document.createElement("p");
      details.className = "help";
      details.textContent = `${m.program.kind} · Permissions: ${m.capabilities.join(", ")}`;
      row.append(details);
      for (const [operation, label] of [
        [
          entry.enabled ? "disable" : "enable",
          entry.enabled ? "Disable" : "Enable",
        ],
        ...(m.program.kind === "effect"
          ? [["activate", "Activate on selected strip"]]
          : []),
        ["remove", "Remove"],
      ]) {
        const button = document.createElement("button");
        button.type = "button";
        button.textContent = label;
        button.onclick = () =>
          task(async () => {
            plugins = await invoke("plugin_manage", {
              operation,
              id: m.id,
              controller: el("controller").value,
              consent: el("consent").checked,
            });
            renderPlugins();
            el("message").textContent =
              operation === "activate"
                ? "Effect activated. Manual scene changes, Stop or suspension cancel it; Disable restores the ordinary scene."
                : "Plugin updated.";
          });
        row.append(button);
      }
      el("plugins").append(row);
    }
  }
  async function preview(text) {
    pluginText = "";
    const m = await invoke("plugin_manage", { operation: "preview", text });
    pluginText = text;
    el("preview").textContent = JSON.stringify(m, null, 2);
    el("message").textContent =
      `${m.name}: review its program and permissions, then install. Installation leaves it disabled.`;
  }
  function init() {
    for (const [event, label, color] of events) {
      const row = document.createElement("fieldset");
      const legend = document.createElement("legend");
      legend.textContent = label;
      row.append(legend);
      const fields = [
        ["enabled", "Use this event", "checkbox", null, null],
        ["color", "Cue color", "color", null, null],
        ["seconds", "Duration (seconds)", "number", 1, 10],
        ["start", "Zone start (0–1000)", "number", 0, 999],
        ["end", "Zone end (0–1000)", "number", 1, 1000],
      ];
      for (const [key, name, type, min, max] of fields) {
        const lab = document.createElement("label"),
          input = document.createElement("input");
        lab.textContent = name;
        input.id = `extension-${event}-${key}`;
        input.type = type;
        if (min !== null) {
          input.min = min;
          input.max = max;
          input.required = true;
        }
        input.value =
          key === "color"
            ? color
            : key === "seconds"
              ? 2
              : key === "end"
                ? 1000
                : 0;
        lab.append(input);
        row.append(lab);
      }
      el("rules").append(row);
    }
    el("open").onclick = () =>
      task(async () => {
        await save();
        await refresh();
        el("dialog").showModal();
      });
    el("close").onclick = () => el("dialog").close();
    el("game-enable").onclick = () =>
      task(async () => {
        const bad = el("rules").querySelector("input:invalid");
        if (bad) {
          bad.reportValidity();
          return;
        }
        const rules = events
          .filter(([event]) => el(event + "-enabled").checked)
          .map(([event]) => ({
            event,
            color: el(event + "-color").value,
            seconds: Number(el(event + "-seconds").value),
            start: Number(el(event + "-start").value),
            end: Number(el(event + "-end").value),
          }));
        gameStatus(
          await invoke("game_configure", {
            config: { controller: el("controller").value, rules },
          }),
        );
        el("message").textContent =
          "Telemetry enabled. Export the new setup and copy it into the game’s cfg folder before starting the game.";
      });
    el("game-disable").onclick = () =>
      task(async () =>
        gameStatus(await invoke("game_configure", { config: null })),
      );
    el("game-export").onclick = () =>
      task(async () => {
        el("message").textContent =
          "Saved setup: " + (await invoke("game_export"));
      });
    el("file").onchange = () =>
      task(async () => {
        const file = el("file").files[0];
        pluginText = "";
        if (!file) return;
        if (file.size > 32768) throw Error("Plugin limit is 32 KiB.");
        await preview(await file.text());
      });
    el("sample").onclick = () =>
      task(() =>
        preview(
          JSON.stringify({
            apiVersion: 1,
            id: "ocean-wave",
            name: "Ocean wave",
            version: "1.0.0",
            capabilities: ["lighting"],
            program: {
              kind: "effect",
              composition: {
                version: 1,
                durationMs: 6000,
                repeats: 0,
                layers: [
                  {
                    kind: "wave",
                    opacity: 100,
                    stops: [
                      { position: 0, color: "#103080" },
                      { position: 1000, color: "#20ddbb" },
                    ],
                  },
                ],
              },
            },
          }),
        ),
      );
    el("install").onclick = () =>
      task(async () => {
        plugins = await invoke("plugin_manage", {
          operation: "install",
          text: pluginText,
        });
        pluginText = "";
        renderPlugins();
        el("message").textContent =
          "Installed disabled. Enable explicitly when ready.";
      });
    setInterval(() => {
      if (el("dialog").open && !working)
        invoke("game_status")
          .then(gameStatus)
          .catch(() => {});
    }, 3000);
  }
  return { init };
})();
