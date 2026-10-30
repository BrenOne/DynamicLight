// Desktop/updater and standalone CLI assets have separate stable names.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const os = require('node:os');
const { spawnSync } = require('node:child_process');

const bundles = {
  'x86_64-pc-windows-msvc': [['nsis', '.exe', 'dynamiclight-desktop-windows-x64-setup.exe']],
  'aarch64-apple-darwin': [['dmg', '.dmg', 'dynamiclight-desktop-macos-arm64.dmg']],
  'x86_64-apple-darwin': [['dmg', '.dmg', 'dynamiclight-desktop-macos-x64.dmg']],
  'x86_64-unknown-linux-gnu': [
    ['deb', '.deb', 'dynamiclight-desktop-linux-x64.deb'],
    ['appimage', '.AppImage', 'dynamiclight-desktop-linux-x64.AppImage'],
  ],
};
const updates = {
  'x86_64-pc-windows-msvc': ['windows-x86_64', 'nsis', '.exe', 'dynamiclight-desktop-windows-x64-setup.exe'],
  'aarch64-apple-darwin': ['darwin-aarch64', 'macos', '.app.tar.gz', 'dynamiclight-desktop-macos-arm64.app.tar.gz'],
  'x86_64-apple-darwin': ['darwin-x86_64', 'macos', '.app.tar.gz', 'dynamiclight-desktop-macos-x64.app.tar.gz'],
  'x86_64-unknown-linux-gnu': ['linux-x86_64', 'appimage', '.AppImage', 'dynamiclight-desktop-linux-x64.AppImage'],
};
const cli = {
  'x86_64-pc-windows-msvc': 'dynamiclight-cli-windows-x64.exe',
  'aarch64-apple-darwin': 'dynamiclight-cli-macos-arm64',
  'x86_64-apple-darwin': 'dynamiclight-cli-macos-x64',
  'x86_64-unknown-linux-gnu': 'dynamiclight-cli-linux-x64',
};
const minisign = process.env.MINISIGN || (process.platform === 'win32' ? path.resolve(__dirname, '../.local/minisign/minisign-win64/x86_64/minisign.exe') : 'minisign');
function verifySignature(file, signature, publicKey, version) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'dynamiclight-verify-'));
  try {
    fs.writeFileSync(path.join(dir, 'key.pub'), Buffer.from(publicKey, 'base64'));
    fs.writeFileSync(path.join(dir, 'signature'), Buffer.from(signature, 'base64'));
    const result = spawnSync(minisign, ['-V', '-m', file, '-p', path.join(dir, 'key.pub'), '-x', path.join(dir, 'signature')], { encoding: 'utf8' });
    if (result.error || result.status !== 0) throw new Error(`Invalid updater signature for ${path.basename(file)} (minisign must be installed)`);
    const comment = Buffer.from(signature, 'base64').toString('utf8').split(/\r?\n/)[2];
    const signedVersion = comment?.replace(/^trusted comment: /, '').split('\t').find(field => field.startsWith('version:'))?.slice(8);
    if (signedVersion !== version) throw new Error(`Updater signature version must match ${version}`);
  } finally { fs.rmSync(dir, { recursive: true, force: true }); }
}
const versionPattern = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z]+(?:[.-][0-9A-Za-z]+)*)?$/;

function version(root, tag) {
  const read = name => fs.readFileSync(path.join(root, name), 'utf8');
  const value = JSON.parse(read('package.json')).version;
  const cargoVersion = read('src-tauri/Cargo.toml').match(/^version = "([^"]+)"/m)?.[1];
  const lockVersion = read('src-tauri/Cargo.lock').match(/\[\[package\]\]\r?\nname = "dynamiclight"\r?\nversion = "([^"]+)"/)?.[1];
  const cliVersion = read('control/Cargo.toml').match(/^version = "([^"]+)"/m)?.[1];
  const cliLockVersion = read('control/Cargo.lock').match(/\[\[package\]\]\r?\nname = "dynamiclight-control"\r?\nversion = "([^"]+)"/)?.[1];
  if (!versionPattern.test(value) || [cargoVersion, lockVersion, cliVersion, cliLockVersion, JSON.parse(read('src-tauri/tauri.conf.json')).version].some(v => v !== value)) {
    throw new Error('Versions must agree in package.json, Cargo.toml, Cargo.lock and tauri.conf.json');
  }
  if (tag && tag !== `v${value}`) throw new Error(`Release tag ${tag} must match v${value}`);
  return value;
}

function stage(root, target, output) {
  if (!bundles[target]) throw new Error(`Unsupported target: ${target}`);
  fs.mkdirSync(output, { recursive: true });
  for (const [directory, extension, name] of bundles[target]) {
    const source = path.join(root, 'src-tauri/target', target, 'release/bundle', directory);
    const matches = fs.readdirSync(source).filter(file => file.endsWith(extension));
    if (matches.length !== 1) throw new Error(`Expected one ${extension} in ${source}, got ${matches.length}`);
    const file = path.join(source, matches[0]);
    if (!fs.statSync(file).size) throw new Error(`Empty package: ${file}`);
    fs.copyFileSync(file, path.join(output, name));
  }
  const [, directory, extension, name] = updates[target];
  const source = path.join(root, 'src-tauri/target', target, 'release/bundle', directory);
  const matches = fs.readdirSync(source).filter(file => file.endsWith(extension));
  if (matches.length !== 1) throw new Error(`Expected one updater ${extension} in ${source}`);
  fs.copyFileSync(path.join(source, matches[0]), path.join(output, name));
  fs.copyFileSync(path.join(source, `${matches[0]}.sig`), path.join(output, `${name}.sig`));
  const binary = path.join(root, 'control/target', target, 'release', target.includes('windows') ? 'dynamiclight.exe' : 'dynamiclight');
  if (!fs.statSync(binary).size) throw new Error(`Empty CLI: ${binary}`);
  fs.copyFileSync(binary, path.join(output, cli[target]));
}

function assemble(root, output, tag) {
  const value = version(root, tag);
  const expected = [...new Set([...Object.values(bundles).flat().map(bundle => bundle[2]), ...Object.values(updates).flatMap(u => [u[3], `${u[3]}.sig`]), ...Object.values(cli)])].sort();
  const actual = fs.readdirSync(output).sort();
  if (JSON.stringify(expected) !== JSON.stringify(actual)) throw new Error('Release must contain exactly one package for every supported platform');
  for (const name of expected) if (!fs.statSync(path.join(output, name)).size) throw new Error(`Empty package: ${name}`);
  const publicKey = JSON.parse(fs.readFileSync(path.join(root, 'src-tauri/tauri.conf.json'))).plugins.updater.pubkey;
  const platforms = {};
  for (const [platform, , , name] of Object.values(updates)) {
    const signature = fs.readFileSync(path.join(output, `${name}.sig`), 'utf8').trim();
    verifySignature(path.join(output, name), signature, publicKey, value);
    platforms[platform] = { signature, url: `https://github.com/BrenOne/DynamicLight/releases/download/v${value}/${name}` };
  }
  fs.writeFileSync(path.join(output, 'latest.json'), JSON.stringify({ version: value, notes: fs.readFileSync(path.join(root, 'docs/release-notes.md'), 'utf8'), platforms }, null, 2) + '\n');
  for (const [source, name] of [
    ['scripts/install.sh', 'install.sh'],
    ['scripts/install.ps1', 'install.ps1'],
    ['src-tauri/linux/69-dynamiclight.rules', '69-dynamiclight.rules'],
  ]) fs.copyFileSync(path.join(root, source), path.join(output, name));
  fs.writeFileSync(path.join(output, 'release-version.txt'), `${value}\n`);
  const checksums = fs.readdirSync(output).sort().map(name => {
    const hash = crypto.createHash('sha256').update(fs.readFileSync(path.join(output, name))).digest('hex');
    return `${hash}  ${name}\n`;
  }).join('');
  fs.writeFileSync(path.join(output, 'SHA256SUMS'), checksums);
}

module.exports = { bundles, updates, cli, version, stage, assemble, minisign, verifySignature };
if (require.main === module) {
  const [command, argument, output] = process.argv.slice(2);
  const root = path.resolve(__dirname, '..');
  if (command === 'version') console.log(version(root, argument));
  else if (command === 'stage') stage(root, argument, output);
  else if (command === 'assemble') assemble(root, argument, output);
  else throw new Error('Usage: release.cjs version [tag] | stage <target> <directory> | assemble <directory> [tag]');
}
