// Only trusted release builds receive the repository key. PRs exercise the same
// packaging pipeline with a disposable key that cannot update released apps.
const fs = require('node:fs');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const root = path.resolve(__dirname, '..');
const key = path.join(process.env.RUNNER_TEMP, 'dynamiclight-updater.key');
const config = { bundle: { createUpdaterArtifacts: true } };
if (process.env.RELEASE_BUILD === 'true') {
  if (!process.env.RELEASE_SIGNING_KEY?.trim()) throw new Error('TAURI_SIGNING_PRIVATE_KEY secret is required to publish updates');
  fs.writeFileSync(key, process.env.RELEASE_SIGNING_KEY.trim(), { mode: 0o600 });
} else {
  const result = spawnSync(process.execPath, [path.join(root, 'node_modules/@tauri-apps/cli/tauri.js'), 'signer', 'generate', '--ci', '-w', key], { encoding: 'utf8' });
  if (result.status !== 0) throw new Error('Unable to create disposable PR signing key');
  config.plugins = { updater: { pubkey: fs.readFileSync(`${key}.pub`, 'utf8').trim() } };
}
fs.mkdirSync(path.join(root, '.local'), { recursive: true });
fs.writeFileSync(path.join(root, '.local/updater-build.json'), JSON.stringify(config));
fs.appendFileSync(process.env.GITHUB_ENV, `TAURI_SIGNING_PRIVATE_KEY=${key}\nTAURI_SIGNING_PRIVATE_KEY_PASSWORD=\n`);
