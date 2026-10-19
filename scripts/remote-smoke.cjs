// Called only after studio-smoke verifies the isolated native app identifier.
const assert = require("node:assert/strict");
const https = require("node:https");
const crypto = require("node:crypto");
const { chromium } = require("playwright");
module.exports = async function remoteSmoke(invoke) {
  let browser;
  const original = (await invoke("control", { action: { type: "snapshot" } }))
    .settings;
  await invoke("control", {
    action: {
      type: "settings",
      settings: { ...original, offOnLock: false, offOnDisplaySleep: false },
    },
  });
  try {
    const state = await invoke("remote_manage", {
      operation: "enable",
      address: "127.0.0.1",
      controller: "primary",
    });
    assert(state.enabled && new URL(state.endpoint).hostname === "127.0.0.1");
    const certificate = await invoke("remote_manage", {
      operation: "certificate",
    });
    assert.equal(
      new crypto.X509Certificate(certificate.pem).fingerprint256,
      state.fingerprint,
    );
    const call = (path, data, cookie = "", origin = state.endpoint) =>
      new Promise((resolve, reject) => {
        const body = JSON.stringify(data);
        const req = https.request(
          state.endpoint + path,
          {
            method: "POST",
            ca: certificate.pem,
            agent: false,
            headers: {
              Origin: origin,
              "Content-Type": "application/json",
              "Content-Length": Buffer.byteLength(body),
              ...(cookie ? { Cookie: cookie } : {}),
            },
          },
          (res) => {
            let text = "";
            res.setEncoding("utf8");
            res.on("data", (chunk) => (text += chunk));
            res.on("end", () =>
              resolve({
                status: res.statusCode,
                headers: res.headers,
                text,
                data: res.headers["content-type"]?.includes("application/json")
                  ? JSON.parse(text)
                  : null,
              }),
            );
          },
        );
        req.setTimeout(6000, () => req.destroy(Error("TLS request timeout")));
        req.on("error", reject);
        req.end(body);
      });
    const command = (command) => ({ version: 1, command });
    assert.equal(
      (await call("/v1/command", command({ command: "status" }))).status,
      401,
    );
    assert.equal(
      (
        await call(
          "/pair",
          { code: state.pairingCode, name: "Bad origin" },
          "",
          "https://example.invalid",
        )
      ).status,
      403,
    );
    assert.equal(
      (await call("/pair", { code: "wrong", name: "Fixture" })).status,
      403,
    );
    const paired = await call("/pair", {
      code: state.pairingCode,
      name: "TLS fixture",
    });
    assert.equal(paired.status, 200);
    const cookie = paired.headers["set-cookie"][0].split(";")[0];
    assert.match(
      paired.headers["set-cookie"][0],
      /Secure; HttpOnly; SameSite=Strict/,
    );
    assert.equal(
      (await call("/pair", { code: state.pairingCode, name: "Replay" })).status,
      403,
    );
    const status = await call(
      "/v1/command",
      command({ command: "status" }),
      cookie,
    );
    assert.equal(status.status, 200);
    assert.equal(status.data.data.requested, false);
    assert(!("colors" in status.data.data));
    assert.equal(
      (
        await call(
          "/v1/command",
          command({ command: "power", state: "on", allowCapture: true }),
          cookie,
        )
      ).status,
      400,
    );
    assert.equal(
      (
        await call(
          "/v1/command",
          command({ command: "brightness", value: 13 }),
          cookie,
        )
      ).data.data.brightness,
      13,
    );
    assert.equal(
      (
        await call(
          "/v1/command",
          command({ command: "color", value: "#113355" }),
          cookie,
        )
      ).status,
      200,
    );
    let snapshot = await invoke("control", { action: { type: "snapshot" } });
    assert.equal(snapshot.settings.color, "#113355");
    assert(
      !snapshot.requested && !snapshot.capturing && !snapshot.status.connected,
    );
    const view = await invoke("remote_manage", { operation: "status" });
    assert.equal(view.clients[0].name, "TLS fixture");
    await invoke("remote_manage", {
      operation: "revoke",
      client: view.clients[0].id,
    });
    assert.equal(
      (
        await call(
          "/v1/command",
          command({ command: "brightness", value: 99 }),
          cookie,
        )
      ).status,
      401,
    );
    assert.equal(
      (await invoke("control", { action: { type: "snapshot" } })).settings
        .brightness,
      13,
    );

    const pairing = await invoke("remote_manage", { operation: "pair" });
    browser = await chromium.launch({
      headless: true,
      channel: process.env.UI_BROWSER_CHANNEL || "msedge",
    });
    // Trust exception is confined to a disposable localhost browser test context.
    // The requests above verify the actual certificate and IP SAN using Node TLS.
    const context = await browser.newContext({
      ignoreHTTPSErrors: true,
      viewport: { width: 390, height: 844 },
    });
    const page = await context.newPage();
    const errors = [];
    page.on("pageerror", (e) => errors.push(e.message));
    await page.goto(state.endpoint);
    await page.locator("#name").fill("Phone fixture");
    await page.locator("#code").fill(pairing.pairingCode);
    await page.locator("#pair button").click();
    await page.locator("#controls").waitFor({ state: "visible" });
    await page.locator("#brightness").fill("23");
    await page.locator("#brightness").dispatchEvent("change");
    await page.waitForFunction(
      () => document.querySelector("#level").textContent === "23%",
    );
    await page.locator("#color").fill("#224466");
    await page.locator("#set-color").click();
    await page.locator("#off").click();
    snapshot = await invoke("control", { action: { type: "snapshot" } });
    assert.equal(snapshot.settings.brightness, 23);
    assert.equal(snapshot.settings.color, "#224466");
    assert(!snapshot.requested);
    assert(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    );
    await page.waitForFunction(() => !document.querySelector("#off").disabled);
    await page.screenshot({ path: ".local/phone-remote.png", fullPage: true });
    const clients = (await invoke("remote_manage", { operation: "status" }))
      .clients;
    await invoke("remote_manage", {
      operation: "revoke",
      client: clients.find((c) => c.name === "Phone fixture").id,
    });
    await page.locator("#retry").click();
    await page.locator("#pair").waitFor({ state: "visible" });
    assert.deepEqual(errors, []);
    console.log(
      "Native HTTPS remote passed: certificate/IP verification, Origin/auth rejection, one-use pairing, restricted commands, phone UI, revocation and off-state preservation. Loopback only.",
    );
  } finally {
    await browser?.close();
    await invoke("remote_manage", { operation: "disable" });
    await invoke("control", {
      action: { type: "settings", settings: original },
    });
  }
};
