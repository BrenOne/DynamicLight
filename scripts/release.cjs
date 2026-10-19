// The desktop release contract. CLI assets for #16 can be added independently.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');

const bundles = {
  'x86_64-pc-windows-msvc': [['nsis', '.exe', 'dynamiclight-desktop-windows-x64-setup.exe']],
  'aarch64-apple-darwin': [['dmg', '.dmg', 'dynamiclight-desktop-macos-arm64.dmg']],
  'x86_64-apple-darwin': [['dmg', '.dmg', 'dynamiclight-desktop-macos-x64.dmg']],
  'x86_64-unknown-linux-gnu': [
    ['deb', '.deb', 'dynamiclight-desktop-linux-x64.deb'],
    ['appimage', '.AppImage', 'dynamiclight-desktop-linux-x64.AppImage'],
  ],
};
const versionPattern = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z]+(?:[.-][0-9A-Za-z]+)*)?$/;

function version(root, tag) {
  const read = name => fs.readFileSync(path.join(root, name), 'utf8');
  const value = JSON.parse(read('package.json')).version;
  const cargoVersion = read('src-tauri/Cargo.toml').match(/^version = "([^"]+)"/m)?.[1];
  const lockVersion = read('src-tauri/Cargo.lock').match(/\[\[package\]\]\r?\nname = "dynamiclight"\r?\nversion = "([^"]+)"/)?.[1];
  if (!versionPattern.test(value) || [cargoVersion, lockVersion, JSON.parse(read('src-tauri/tauri.conf.json')).version].some(v => v !== value)) {
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
}

function assemble(root, output, tag) {
  const value = version(root, tag);
  const expected = Object.values(bundles).flat().map(bundle => bundle[2]).sort();
  const actual = fs.readdirSync(output).sort();
  if (JSON.stringify(expected) !== JSON.stringify(actual)) throw new Error('Release must contain exactly one package for every supported platform');
  for (const name of expected) if (!fs.statSync(path.join(output, name)).size) throw new Error(`Empty package: ${name}`);
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

module.exports = { bundles, version, stage, assemble };
if (require.main === module) {
  const [command, argument, output] = process.argv.slice(2);
  const root = path.resolve(__dirname, '..');
  if (command === 'version') console.log(version(root, argument));
  else if (command === 'stage') stage(root, argument, output);
  else if (command === 'assemble') assemble(root, argument, output);
  else throw new Error('Usage: release.cjs version [tag] | stage <target> <directory> | assemble <directory> [tag]');
}
