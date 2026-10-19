/* Update state and trust decisions live in Rust; this UI only requests actions. */
window.Updates = (() => {
  const $ = (id) => document.getElementById(id);
  const invoke = (command, args) => window.__TAURI__.core.invoke(command, args);
  let state;
  function render(next) {
    state = next;
    const working = ["checking", "downloading", "installing"].includes(state.phase);
    const available = Boolean(state.version);
    $("update-automatic").checked = state.automatic;
    $("update-check").disabled = working;
    $("update-install").hidden = !available || !state.canInstall;
    $("update-install").disabled = working;
    $("update-banner").hidden = !available;
    $("update-banner").textContent = `Update ${state.version} available`;
    $("update-progress").hidden = state.phase !== "downloading";
    if (state.progress == null) $("update-progress").removeAttribute("value");
    else $("update-progress").value = state.progress;
    const messages = {
      idle: `Installed version ${state.currentVersion}.`,
      checking: "Checking GitHub for updates…",
      current: `Version ${state.currentVersion} is up to date.`,
      available: `Version ${state.version} is available.`,
      downloading: `Downloading and verifying ${state.version}…${state.progress == null ? "" : ` ${state.progress}%`}`,
      installing: "Installing update. DynamicLight will restart…",
      error: state.message,
    };
    $("update-status").textContent = state.message || messages[state.phase];
    $("update-manual").hidden = state.canInstall;
    $("update-notes").textContent = state.notes;
    $("update-notes-details").hidden = !state.notes;
  }
  async function action(command, args) {
    try { render(await invoke(command, args)); }
    catch (error) {
      if (state) render(state);
      $("update-status").textContent = String(error);
    }
  }
  async function init({ save, showSettings }) {
    $("update-check").onclick = () => action("update_check");
    $("update-automatic").onchange = () => action("update_automatic", { enabled: $("update-automatic").checked });
    $("update-banner").onclick = showSettings;
    $("update-install").onclick = () => $("update-dialog").showModal();
    $("update-cancel").onclick = () => $("update-dialog").close();
    $("update-confirm").onclick = async () => {
      $("update-confirm").disabled = true;
      try {
        await save();
        $("update-dialog").close();
        await action("update_install");
      } catch (error) {
        $("update-dialog-error").textContent = `Could not save settings: ${error}`;
      } finally { $("update-confirm").disabled = false; }
    };
    await window.__TAURI__.event.listen("update-status", (event) => render(event.payload));
    await action("update_status");
  }
  return { init };
})();
