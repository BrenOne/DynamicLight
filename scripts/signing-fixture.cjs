// Real disposable minisign signatures for release tests. Never uses the release key.
const fs = require('node:fs');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const { minisign } = require('./release.cjs');
module.exports = function signingFixture(root) {
  const key = path.join(root, 'test-signing.key');
  function run(args) {
    const result = spawnSync(minisign, args, { encoding: 'utf8' });
    if (result.error || result.status !== 0) throw new Error(`Fixture signing failed: ${result.error || result.stderr}`);
  }
  run(['-G', '-W', '-s', key, '-p', `${key}.pub`]);
  const publicKey = Buffer.from(fs.readFileSync(`${key}.pub`)).toString('base64');
  const configPath = path.join(root, 'src-tauri/tauri.conf.json');
  const config = JSON.parse(fs.readFileSync(configPath));
  config.plugins = { updater: { pubkey: publicKey } };
  fs.writeFileSync(configPath, JSON.stringify(config));
  return file => {
    const signature = `${file}.minisig`;
    run(['-S', '-s', key, '-m', file, '-x', signature, '-t', `version:${config.version}`]);
    fs.writeFileSync(`${file}.sig`, Buffer.from(fs.readFileSync(signature)).toString('base64'));
    fs.unlinkSync(signature);
  };
};
