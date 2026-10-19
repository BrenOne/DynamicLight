"use strict";
window.Appearance = (() => {
  const root = document.documentElement;
  const media = matchMedia("(prefers-color-scheme: dark)");
  let current = { theme: "system", accent: "violet", density: "spacious" };
  function render(value) {
    current = value;
    root.dataset.theme =
      value.theme === "system"
        ? media.matches
          ? "dark"
          : "light"
        : value.theme;
    root.dataset.accent = value.accent;
    root.dataset.density = value.density;
    for (const key of ["theme", "accent", "density"]) {
      const field = document.getElementById("appearance-" + key);
      if (field) field.value = value[key];
    }
  }
  media.addEventListener("change", () => render(current));
  async function init() {
    const status = document.getElementById("appearance-status");
    try {
      await window.__TAURI__.event.listen("appearance-changed", ({ payload }) =>
        render(payload),
      );
      render(await window.__TAURI__.core.invoke("appearance", { value: null }));
      const fields = ["theme", "accent", "density"].map((k) =>
        document.getElementById("appearance-" + k),
      );
      for (const field of fields.filter(Boolean))
        field.onchange = async () => {
          const value = Object.fromEntries(
            fields.map((f) => [f.id.slice(11), f.value]),
          );
          fields.forEach((f) => {
            f.disabled = true;
          });
          try {
            render(await window.__TAURI__.core.invoke("appearance", { value }));
            status.textContent = "Appearance saved.";
          } catch (error) {
            render(current);
            status.textContent = String(error);
          } finally {
            fields.forEach((f) => {
              f.disabled = false;
            });
          }
        };
    } catch (error) {
      if (status) status.textContent = String(error);
    }
  }
  render(current);
  init();
  return { render };
})();
