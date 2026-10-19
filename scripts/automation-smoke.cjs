// Native Windows integration check. Build with an isolated identifier first:
// pnpm tauri build --debug --no-bundle --config .local/automation-smoke.json
// config: {"identifier":"com.dynamiclight.automation-test","bundle":{"createUpdaterArtifacts":false}}
// No commands in this test may start USB or capture.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const http = require('node:http');
const { spawn, spawnSync } = require('node:child_process');
const { chromium } = require('playwright');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const connectionFile = path.join(process.env.APPDATA, 'com.dynamiclight.automation-test/automation/session.json');
const cliBinary = path.resolve('control/target/debug/dynamiclight.exe');
function cli(args, expected = 0) {
  const result = spawnSync(cliBinary, ['--connection', connectionFile, '--json', ...args], { encoding:'utf8', timeout:15000 });
  assert.equal(result.status, expected, result.stderr);
  return JSON.parse(result.stdout);
}
function request(connection, command, extra = {}) {
  const body = JSON.stringify({version:1, command});
  return new Promise((resolve, reject) => {
    const req = http.request({hostname:'127.0.0.1', port:connection.port, path:'/v1/command', method:'POST', headers:{Authorization:`Bearer ${connection.token}`, 'Content-Type':'application/json', 'Content-Length':Buffer.byteLength(body), ...extra}}, res => {
      const chunks = [];
      res.on('data', c => chunks.push(c));
      res.on('end', () => resolve({code:res.statusCode, body:JSON.parse(Buffer.concat(chunks))}));
    });
    req.on('error', reject);
    req.setTimeout(5000, () => req.destroy(Error('HTTP timeout')));
    req.end(body);
  });
}
(async () => {
  const child = spawn(path.resolve('src-tauri/target/debug/dynamiclight.exe'), [], {env:{...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:'--remote-debugging-port=9225'}, stdio:'ignore'});
  let browser, page, isolated = false;
  try {
    for (let i=0; i<60; i++) {
      if (child.exitCode !== null) throw Error('Test app exited; use an isolated build');
      try { browser = await chromium.connectOverCDP('http://127.0.0.1:9225'); break; } catch { await delay(250); }
    }
    assert(browser);
    for (let i=0; i<40; i++) {
      page = browser.contexts()[0].pages().find(p => p.url().includes('tauri.localhost'));
      if (page) break;
      await delay(250);
    }
    await page.waitForFunction(() => typeof settings !== 'undefined' && settings);
    const invoke = (command, args) => page.evaluate(({command,args}) => window.__TAURI__.core.invoke(command,args), {command,args});
    const initial = await invoke('automation_status');
    assert.equal(path.resolve(initial.connectionFile), path.resolve(connectionFile), 'Refusing to modify a non-test installation');
    isolated = true;
    const snapshot = await invoke('control', {action:{type:'snapshot'}});
    assert(!snapshot.requested && !snapshot.status.connected && !snapshot.capturing);
    await invoke('automation_configure', {enabled:true,allowCapture:false});
    const first = JSON.parse(fs.readFileSync(connectionFile));
    assert.equal(cli(['status']).data.connected, false);
    assert.equal(cli(['brightness','27']).data.brightness,27);
    assert.equal(cli(['color','#335577']).data.mode,'solid');
    const colored = await invoke('control',{action:{type:'snapshot'}});
    assert.equal(colored.settings.color,'#335577');
    assert(!colored.requested && !colored.capturing && !colored.status.connected);
    assert(cli(['scenes','list']).data.some(s => s.id === 'cinema'));
    assert(Array.isArray(cli(['profiles','list']).data));
    assert.match(cli(['scenes','apply','cinema'],1).error, /Capture requires/);
    assert.match(cli(['scenes','apply','cinema','--allow-capture'],1).error, /Capture requires/);
    assert.equal((await request(first,{command:'status'},{Authorization:'Bearer wrong'})).code,401);
    assert.equal((await request(first,{command:'status'},{Origin:'https://example.com'})).code,403);
    assert.equal((await request(first,{command:'brightness',value:101})).code,400);
    const results = await Promise.all(Array.from({length:35},() => request(first,{command:'status'})));
    assert(results.some(r => r.code === 429));
    const configured = await invoke('automation_configure',{enabled:true,allowCapture:true});
    assert(configured.allowCapture);
    const second = JSON.parse(fs.readFileSync(connectionFile));
    assert(first.token !== second.token);
    assert.equal((await request({...second,token:first.token},{command:'status'})).code,401);
    assert.match(cli(['scenes','apply','cinema'],1).error, /Capture requires/);
    await invoke('control',{action:{type:'selectMode',mode:'screen'}});
    assert.match(cli(['power','on'],1).error, /Capture requires/);
    assert.equal(cli(['power','off']).data.requested,false);
    const final = await invoke('control',{action:{type:'snapshot'}});
    assert(!final.requested && !final.status.connected && !final.capturing);
    await invoke('automation_configure',{enabled:false,allowCapture:false});
    assert(!fs.existsSync(connectionFile));
    assert.equal(cli(['status'],1).ok,false);
    console.log('Native automation + console CLI passed: status, brightness, lists, consent, auth, browser rejection, validation, rate limit, rotation, revocation; no USB/capture.');
  } finally {
    if (page && isolated) {
      await page.evaluate(() => window.__TAURI__.core.invoke('automation_configure',{enabled:false,allowCapture:false})).catch(()=>{});
      await page.evaluate(() => window.__TAURI__.core.invoke('quit')).catch(()=>{});
    }
    for (let i=0;i<20 && child.exitCode===null;i++) await delay(250);
    if (child.exitCode===null) child.kill();
    await browser?.close();
  }
})().catch(error => { console.error(error); process.exitCode=1; });
