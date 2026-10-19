const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const crypto = require('node:crypto');
const { bundles, version, stage, assemble } = require('./release.cjs');
const root = path.resolve(__dirname, '..');

function fixture(t) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'dynamiclight-release-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  for (const file of ['package.json', 'src-tauri/Cargo.toml', 'src-tauri/Cargo.lock', 'src-tauri/tauri.conf.json', 'scripts/install.sh', 'scripts/install.ps1', 'src-tauri/linux/69-dynamiclight.rules']) {
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
  for (const [target, files] of Object.entries(bundles)) {
    for (const [folder, ext] of files) {
      const source = path.join(dir, 'src-tauri/target', target, 'release/bundle', folder);
      fs.mkdirSync(source, { recursive: true });
      fs.writeFileSync(path.join(source, `upstream-versioned-name${ext}`), `fixture ${target} ${ext}`);
    }
    stage(dir, target, output);
  }
  assemble(dir, output, `v${version(dir)}`);
  const checksums = fs.readFileSync(path.join(output, 'SHA256SUMS'), 'utf8').trim().split('\n');
  assert.equal(checksums.length, 9);
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
