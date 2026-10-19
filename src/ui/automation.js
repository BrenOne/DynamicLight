window.Automation = (() => {
  const $ = (id) => document.getElementById(id);
  let state;
  function render(next) {
    state = next;
    $("automation-enabled").checked = state.enabled;
    $("automation-capture").checked = state.allowCapture;
    $("automation-capture").disabled = !state.enabled;
    $("automation-rotate").disabled = !state.enabled;
    $("automation-connection").hidden = !state.enabled;
    $("automation-endpoint").textContent = state.endpoint || "";
    $("automation-file").textContent = state.connectionFile;
    $("automation-status").textContent = state.message || (state.enabled ? "Local automation is enabled. The CLI can control this app while it is running." : "Local automation is off.");
  }
  async function configure() {
    const controls = ["automation-enabled", "automation-capture", "automation-rotate"].map($);
    const args = { enabled: $("automation-enabled").checked, allowCapture: $("automation-capture").checked };
    controls.forEach((c) => { c.disabled = true; });
    try { render(await window.__TAURI__.core.invoke("automation_configure", args)); }
    catch (error) { if (state) render(state); $("automation-status").textContent = String(error); }
    finally { $("automation-enabled").disabled = false; }
  }
  async function init() {
    $("automation-enabled").onchange = configure;
    $("automation-capture").onchange = configure;
    $("automation-rotate").onclick = configure;
    try { render(await window.__TAURI__.core.invoke("automation_status")); }
    catch (error) { $("automation-status").textContent = String(error); }
  }
  return { init };
})();
