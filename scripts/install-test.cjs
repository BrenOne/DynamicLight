// Exercise the real installers with fake download/process tools. No network,
// package managers, USB access, or installed applications are touched.
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const crypto = require('node:crypto');
const { spawnSync } = require('node:child_process');
const { bundles } = require('./release.cjs');
const root = path.resolve(__dirname, '..');
const windows = process.platform === 'win32';

function fixture(t, scenario = {}) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'dynamiclight-install-test-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  fs.mkdirSync(path.join(dir, 'bin'));
  fs.mkdirSync(path.join(dir, 'home'));
  const names = [...Object.values(bundles).flat().map(b => b[2]), '69-dynamiclight.rules'];
  const checksums = names.map(name => {
    const contents = `package ${name}`;
    fs.writeFileSync(path.join(dir, name), contents);
    return `${crypto.createHash('sha256').update(contents).digest('hex')}  ${name}\n`;
  }).join('');
  fs.writeFileSync(path.join(dir, 'SHA256SUMS'), scenario.missing ? '' : checksums + (scenario.duplicate ? checksums : ''));
  fs.writeFileSync(path.join(dir, 'release-version.txt'), '0.7.0\n');
  if (scenario.corrupt) for (const name of names) fs.appendFileSync(path.join(dir, name), 'corrupt');
  return dir;
}

function run(t, scenario = {}) {
  const dir = fixture(t, scenario);
  const env = { ...process.env, FIXTURE: dir, FAIL_DOWNLOAD: scenario.network ? '1' : '', INSTALL_EXIT: String(scenario.exit || 0) };
  let result;
  if (windows) {
    // Let Windows PowerShell discover its own modules instead of inheriting
    // a PowerShell 7 host's incompatible module search path.
    delete env.PSModulePath;
    const harness = path.join(dir, 'harness.ps1');
    fs.writeFileSync(harness, `\uFEFF
$ErrorActionPreference = 'Stop'
$env:OS = 'Windows_NT'
$env:PROCESSOR_ARCHITEW6432 = ''
$env:PROCESSOR_ARCHITECTURE = '${scenario.arch || 'AMD64'}'
function Invoke-WebRequest {
    param($Uri, $OutFile, [switch]$UseBasicParsing)
    if ($env:FAIL_DOWNLOAD) { throw 'HTTP 404' }
    Add-Content -LiteralPath (Join-Path $env:FIXTURE 'urls') -Value $Uri
    Copy-Item -LiteralPath (Join-Path $env:FIXTURE ($Uri.Split('/')[-1])) -Destination $OutFile
}
function Start-Process {
    param($FilePath, $ArgumentList, [switch]$Wait, [switch]$PassThru, $WindowStyle)
    if (($ArgumentList -join ' ') -ne '/S' -or !$Wait -or $WindowStyle -ne 'Hidden') { throw 'Incorrect NSIS invocation' }
    Set-Content -LiteralPath (Join-Path $env:FIXTURE 'installed') -Value $FilePath
    return [pscustomobject]@{ ExitCode = [int]$env:INSTALL_EXIT }
}
& '${path.join(root, 'scripts/install.ps1').replaceAll("'", "''")}' ${scenario.install ? '' : '-DownloadOnly'} -Directory (Join-Path $env:FIXTURE 'downloads') ${scenario.version ? `-Version '${scenario.version}'` : ''}
`);
    result = spawnSync('powershell.exe', ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', harness], { env, encoding: 'utf8' });
  } else {
    Object.assign(env, {
      PATH: `${path.join(dir, 'bin')}:${process.env.PATH}`,
      HOME: path.join(dir, 'home'), TEST_OS: scenario.os || 'Linux', TEST_ARCH: scenario.arch || 'x86_64',
      TEST_ROSETTA: scenario.rosetta ? '1' : '0',
    });
    const mocks = {
      uname: 'case "$1" in -s) echo "$TEST_OS" ;; -m) echo "$TEST_ARCH" ;; esac',
      sysctl: 'echo "$TEST_ROSETTA"',
      id: 'echo 1000',
      curl: `
[ -z "$FAIL_DOWNLOAD" ] || exit 22
while [ "$#" -gt 0 ]; do
  case "$1" in https:*) url=$1; shift ;; -o) output=$2; shift 2 ;; *) shift ;; esac
done
echo "$url" >> "$FIXTURE/urls"
cp "$FIXTURE/$(basename "$url")" "$output"`,
      sudo: '"$@"',
      'apt-get': 'echo "$*" > "$FIXTURE/installed"; exit "$INSTALL_EXIT"',
      hdiutil: `
if [ "$1" = attach ]; then
  while [ "$1" != -mountpoint ]; do shift; done
  mkdir -p "$2/DynamicLight.app"
  echo app > "$2/DynamicLight.app/binary"
fi`,
      ditto: 'cp -R "$1" "$2"',
    };
    for (const [name, script] of Object.entries(mocks)) fs.writeFileSync(path.join(dir, 'bin', name), `#!/usr/bin/env bash\nset -eu\n${script}\n`, { mode: 0o755 });
    const args = [path.join(root, 'scripts/install.sh'), '--directory', path.join(dir, 'downloads')];
    if (!scenario.install) args.push('--download-only');
    if (scenario.version) args.push('--version', scenario.version);
    if (scenario.appimage) args.push('--appimage');
    result = spawnSync('bash', args, { env, encoding: 'utf8' });
  }
  assert.ifError(result.error);
  return { ...result, dir, log: result.stdout + result.stderr };
}

test('latest resolves once and downloads verified platform package from a pinned tag', t => {
  const result = run(t);
  assert.equal(result.status, 0, result.log);
  const urls = fs.readFileSync(path.join(result.dir, 'urls'), 'utf8').trim().split(/\r?\n/);
  assert.equal(urls.filter(url => url.includes('/latest/')).length, 1);
  assert.ok(urls.slice(1).every(url => url.includes('/download/v0.7.0/')));
  assert.equal(fs.readdirSync(path.join(result.dir, 'downloads')).length, 1);
  assert.ok(!fs.existsSync(path.join(result.dir, 'installed')));
});

test('explicit prerelease uses the exact requested tag', t => {
  const result = run(t, { version: 'v0.8.0-beta.1' });
  assert.equal(result.status, 0, result.log);
  assert.ok(fs.readFileSync(path.join(result.dir, 'urls'), 'utf8').trim().split(/\r?\n/).every(url => url.includes('/download/v0.8.0-beta.1/')));
});

for (const scenario of [{ corrupt: true }, { missing: true }, { duplicate: true }, { network: true }, { arch: 'armv7' }, { version: '../../bad' }]) {
  test(`fails closed without installing: ${JSON.stringify(scenario)}`, t => {
    const result = run(t, { ...scenario, install: true });
    assert.notEqual(result.status, 0, result.log);
    const message = scenario.corrupt ? /SHA-256 mismatch/ : scenario.missing || scenario.duplicate ? /Missing or ambiguous SHA-256/ : scenario.network ? /HTTP 404|curl/ : scenario.arch ? /No desktop package/ : /Invalid release version/;
    if (!scenario.network || windows) assert.match(result.log, message);
    assert.ok(!fs.existsSync(path.join(result.dir, 'installed')));
    assert.ok(!fs.existsSync(path.join(result.dir, 'downloads')));
  });
}

test('invokes platform installer and propagates installation failure', t => {
  const good = run(t, { install: true });
  assert.equal(good.status, 0, good.log);
  assert.ok(fs.existsSync(path.join(good.dir, 'installed')));
  const bad = run(t, { install: true, exit: 7 });
  assert.notEqual(bad.status, 0, bad.log);
});

if (!windows) {
  for (const scenario of [{ os: 'Darwin', arch: 'arm64' }, { os: 'Darwin', arch: 'x86_64' }, { os: 'Darwin', arch: 'x86_64', rosetta: true }, { appimage: true }]) {
    test(`selects package: ${JSON.stringify(scenario)}`, t => {
      const result = run(t, scenario);
      assert.equal(result.status, 0, result.log);
      const names = fs.readdirSync(path.join(result.dir, 'downloads'));
      const expected = scenario.appimage ? 'dynamiclight-desktop-linux-x64.AppImage' : `dynamiclight-desktop-macos-${scenario.arch === 'arm64' || scenario.rosetta ? 'arm64' : 'x64'}.dmg`;
      assert.ok(names.includes(expected));
      if (scenario.appimage) assert.ok(names.includes('69-dynamiclight.rules'));
    });
  }
  test('AppImage installation uses its own desktop directory and retains the USB rule', t => {
    const result = run(t, { appimage: true, install: true });
    assert.equal(result.status, 0, result.log);
    assert.ok(fs.existsSync(path.join(result.dir, 'home/.local/opt/dynamiclight-desktop/DynamicLight.AppImage')));
    assert.ok(fs.existsSync(path.join(result.dir, 'home/.local/opt/dynamiclight-desktop/69-dynamiclight.rules')));
    assert.ok(!fs.existsSync(path.join(result.dir, 'home/.local/bin/dynamiclight')));
  });
  test('Mac installation copies the app into user Applications', t => {
    const result = run(t, { os: 'Darwin', install: true });
    assert.equal(result.status, 0, result.log);
    assert.equal(fs.readFileSync(path.join(result.dir, 'home/Applications/DynamicLight.app/binary'), 'utf8'), 'app\n');
  });
}
