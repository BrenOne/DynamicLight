const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const crypto = require('node:crypto');
const { bundles, updates, version, stage, assemble, verifySignature } = require('./release.cjs');
const signingFixture = require('./signing-fixture.cjs');
const root = path.resolve(__dirname, '..');

function fixture(t) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'dynamiclight-release-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  for (const file of ['package.json', 'src-tauri/Cargo.toml', 'src-tauri/Cargo.lock', 'src-tauri/tauri.conf.json', 'scripts/install.sh', 'scripts/install.ps1', 'src-tauri/linux/69-dynamiclight.rules', 'docs/release-notes.md']) {
    fs.mkdirSync(path.dirname(path.join(dir, file)), { recursive: true });
    fs.copyFileSync(path.join(root, file), path.join(dir, file));
  }
  return dir;
}

test('release version rejects tag mismatch and manifest drift', t => {
  const dir = fixture(t);
  const current = version(dir);
  assert.equal(version(dir, `v${current}`), current);
  assert.throws(() => version(dir, 'v99.0.0'), /must match/);
  const file = path.join(dir, 'src-tauri/tauri.conf.json');
  const config = JSON.parse(fs.readFileSync(file));
  config.version = '99.0.0';
  fs.writeFileSync(file, JSON.stringify(config));
  assert.throws(() => version(dir), /Versions must agree/);
});

test('stages all platform bundles and hashes exactly the complete release', t => {
  const dir = fixture(t);
  const output = path.join(dir, 'dist');
  const sign = signingFixture(dir);
  for (const [target, files] of Object.entries(bundles)) {
    for (const [folder, ext] of files) {
      const source = path.join(dir, 'src-tauri/target', target, 'release/bundle', folder);
      fs.mkdirSync(source, { recursive: true });
      fs.writeFileSync(path.join(source, `upstream-versioned-name${ext}`), `fixture ${target} ${ext}`);
    }
    const [, folder, ext] = updates[target];
    const source = path.join(dir, 'src-tauri/target', target, 'release/bundle', folder);
    fs.mkdirSync(source, { recursive: true });
    const file = path.join(source, `upstream-versioned-name${ext}`);
    fs.writeFileSync(file, `fixture ${target} ${ext}`);
    sign(file);
    stage(dir, target, output);
  }
  assemble(dir, output, `v${version(dir)}`);
  const checksums = fs.readFileSync(path.join(output, 'SHA256SUMS'), 'utf8').trim().split('\n');
  assert.equal(checksums.length, 16);
  const manifest = JSON.parse(fs.readFileSync(path.join(output, 'latest.json')));
  assert.equal(manifest.version, version(dir));
  assert.equal(Object.keys(manifest.platforms).length, 4);
  for (const platform of Object.values(manifest.platforms)) {
    assert.ok(platform.url.startsWith(`https://github.com/mskutin/DynamicLight/releases/download/v${version(dir)}/`));
    assert.ok(platform.signature.length > 100);
  }
  for (const line of checksums) {
    const [hash, file] = line.split('  ');
    assert.equal(crypto.createHash('sha256').update(fs.readFileSync(path.join(output, file))).digest('hex'), hash);
  }
  assert.equal(fs.readFileSync(path.join(output, 'release-version.txt'), 'utf8'), `${version(dir)}\n`);
});

test('partial, empty and ambiguous packages cannot become a release', t => {
  const dir = fixture(t);
  const output = path.join(dir, 'dist');
  fs.mkdirSync(output);
  assert.throws(() => assemble(dir, output), /every supported platform/);
  assert.throws(() => stage(dir, 'unknown', output), /Unsupported target/);
  const source = path.join(dir, 'src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis');
  fs.mkdirSync(source, { recursive: true });
  fs.writeFileSync(path.join(source, 'one.exe'), '');
  assert.throws(() => stage(dir, 'x86_64-pc-windows-msvc', output), /Empty package/);
  fs.writeFileSync(path.join(source, 'two.exe'), 'two');
  assert.throws(() => stage(dir, 'x86_64-pc-windows-msvc', output), /Expected one/);
});

test('signatures reject a different key, changed bytes and a relabeled version', t => {
  const dir = fixture(t);
  const sign = signingFixture(dir);
  const file = path.join(dir, 'package');
  fs.writeFileSync(file, 'original package');
  sign(file);
  const signature = fs.readFileSync(`${file}.sig`, 'utf8');
  const key = JSON.parse(fs.readFileSync(path.join(dir, 'src-tauri/tauri.conf.json'))).plugins.updater.pubkey;
  verifySignature(file, signature, key, version(dir));
  assert.throws(() => verifySignature(file, signature, key, '99.0.0'), /version must match/);
  const other = fixture(t);
  signingFixture(other);
  const wrongKey = JSON.parse(fs.readFileSync(path.join(other, 'src-tauri/tauri.conf.json'))).plugins.updater.pubkey;
  assert.throws(() => verifySignature(file, signature, wrongKey, version(dir)), /Invalid updater signature/);
  fs.appendFileSync(file, 'changed');
  assert.throws(() => verifySignature(file, signature, key, version(dir)), /Invalid updater signature/);
});
