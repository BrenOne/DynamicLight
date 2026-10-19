"use strict";
window.PhoneRemote = (() => {
  const el = (id) => document.getElementById("remote-" + id);
  let working = false;
  async function call(operation, extra = {}) {
    return invoke("remote_manage", { operation, ...extra });
  }
  function render(s) {
    el("status").textContent = s.enabled
      ? `Remote enabled for ${s.controller}. Pairings expire after 24 hours or when the app closes.`
      : "Remote disabled. No LAN listener is running.";
    el("endpoint").textContent = s.endpoint || "";
    el("fingerprint").textContent = s.fingerprint || "";
    el("code").textContent = s.pairingCode
      ? `${s.pairingCode} · ${s.pairingSeconds}s remaining`
      : "Pairing closed";
    el("clients").replaceChildren();
    for (const client of s.clients) {
      const row = document.createElement("p"),
        label = document.createElement("span"),
        button = document.createElement("button");
      label.textContent = `${client.name} · ${client.connected ? "Connected" : `Last seen ${client.lastSeenSeconds}s ago`} `;
      button.type = "button";
      button.textContent = "Revoke";
      button.setAttribute("aria-label", `Revoke ${client.name}`);
      button.onclick = () =>
        task(async () => render(await call("revoke", { client: client.id })));
      row.append(label, button);
      el("clients").append(row);
    }
    if (!s.clients.length) el("clients").textContent = "No paired phones.";
  }
  async function load() {
    const [s, fleet] = await Promise.all([
      call("status"),
      invoke("controller_list"),
    ]);
    el("address").replaceChildren(
      ...s.interfaces.map(
        (i) => new Option(`${i.address} · ${i.name}`, i.address),
      ),
    );
    el("address").value =
      s.interfaces.find((i) => i.address !== "127.0.0.1")?.address ||
      "127.0.0.1";
    el("controller").replaceChildren(
      ...fleet.controllers.map((c) => new Option(c.name, c.key)),
    );
    el("controller").value = s.controller || fleet.selected;
    render(s);
  }
  async function task(fn) {
    if (working) return;
    working = true;
    el("dialog")
      .querySelectorAll("button:not(#remote-close)")
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
  function init() {
    el("open").onclick = () =>
      task(async () => {
        await save();
        await load();
        el("dialog").showModal();
      });
    el("close").onclick = () => el("dialog").close();
    el("enable").onclick = () =>
      task(async () => {
        render(
          await call("enable", {
            address: el("address").value,
            controller: el("controller").value,
          }),
        );
        el("message").textContent =
          "Open the HTTPS address on your phone, verify its certificate fingerprint, then enter the pairing code. Only allow your private network if the OS asks about a firewall rule.";
      });
    for (const operation of ["disable", "pair", "revokeAll"])
      el(operation).onclick = () =>
        task(async () => render(await call(operation)));
    el("certificate").onclick = () =>
      task(async () => {
        const s = await call("certificate");
        el("pem").textContent = s.pem;
        el("message").textContent =
          "Public certificate only. Compare its SHA-256 fingerprint with your phone’s certificate details. No private key is exported.";
      });
    setInterval(() => {
      if (el("dialog").open && !working)
        call("status")
          .then(render)
          .catch(() => {});
    }, 3000);
  }
  return { init };
})();
