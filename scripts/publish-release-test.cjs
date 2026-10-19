const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { bundles } = require('./release.cjs');
const { publishRelease } = require('./publish-release.cjs');

function harness(t, options = {}) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'dynamiclight-publish-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const value = options.version || '0.8.1';
  const tag = `v${value}`;
  const sha = 'a'.repeat(40);
  const files = {
    'package.json': JSON.stringify({ version: value }),
    'src-tauri/tauri.conf.json': JSON.stringify({ version: value }),
    'src-tauri/Cargo.toml': `[package]\nname = "dynamiclight"\nversion = "${value}"\n`,
    'src-tauri/Cargo.lock': `[[package]]\nname = "dynamiclight"\nversion = "${value}"\n`,
    'src-tauri/linux/69-dynamiclight.rules': 'USB rule',
    'scripts/install.sh': 'Bash installer', 'scripts/install.ps1': 'PowerShell installer',
    'docs/release-notes.md': 'Release notes',
  };
  for (const [name, content] of Object.entries(files)) {
    fs.mkdirSync(path.dirname(path.join(root, name)), { recursive: true });
    fs.writeFileSync(path.join(root, name), content);
  }
  const calls = [];
  const state = {
    tagCommit: options.tagCommit || null,
    release: options.release ? { id: 7, tag_name: tag, draft: options.release === 'draft' } : null,
    uploadFailure: options.uploadFailure,
  };
  const base = 'repos/owner/repo';
  const ok = value => ({ status: 0, stdout: JSON.stringify(value), stderr: '' });
  const fail = status => ({ status: 1, stdout: '', stderr: `gh: Request failed (HTTP ${status})` });
  function execute(args, input) {
    const body = input ? JSON.parse(input) : undefined;
    calls.push({ args, body });
    if (options.apiFailure) return fail(options.apiFailure);
    if (args[0] === 'release' && args[1] === 'upload') {
      if (state.uploadFailure) return fail(503);
      const uploads = args.slice(3, args.indexOf('--clobber'));
      assert.equal(uploads.length, 10);
      assert.ok(uploads.every(file => fs.existsSync(file)));
      return ok(null);
    }
    assert.equal(args[0], 'api');
    const endpoint = args[1];
    const method = args[args.indexOf('--method') + 1];
    if (endpoint === `${base}/releases?per_page=100`) {
      assert.deepEqual(args.slice(2), ['--paginate', '--slurp']);
      return ok([[], state.release ? [state.release] : []]);
    }
    if (endpoint === `${base}/git/ref/tags/${tag}`) return state.tagCommit
      ? ok({ object: { type: options.annotated ? 'tag' : 'commit', sha: options.annotated ? 'b'.repeat(40) : state.tagCommit } }) : fail(404);
    if (endpoint === `${base}/git/tags/${'b'.repeat(40)}`) return ok({ object: { type: 'commit', sha: state.tagCommit } });
    if (endpoint === `${base}/git/refs` && method === 'POST') {
      assert.equal(body.ref, `refs/tags/${tag}`);
      assert.equal(body.sha, sha);
      state.tagCommit = body.sha;
      return ok({});
    }
    if (endpoint === `${base}/releases` && method === 'POST') {
      assert.equal(body.tag_name, tag);
      assert.equal(body.target_commitish, sha);
      assert.equal(body.draft, true);
      assert.equal(body.generate_release_notes, true);
      state.release = { id: 7, ...body };
      return ok(state.release);
    }
    if (endpoint === `${base}/releases/7` && method === 'PATCH') {
      Object.assign(state.release, body);
      return ok(state.release);
    }
    throw new Error(`Unexpected command: ${args.join(' ')}`);
  }
  let attempt = 0;
  return {
    calls, state, sha,
    run(overrides = {}) {
      const directory = path.join(root, `assets-${attempt++}`);
      fs.mkdirSync(directory);
      const names = Object.values(bundles).flat().map(b => b[2]);
      for (const name of options.incomplete ? names.slice(1) : names) fs.writeFileSync(path.join(directory, name), 'package');
      return publishRelease({ root, directory, repo: 'owner/repo', sha, ref: 'refs/heads/main', event: 'push', ...overrides }, execute);
    },
  };
}

test('main creates an exact-commit tag, uploads into a draft and publishes last', t => {
  const h = harness(t);
  assert.equal(h.run(), 'published');
  assert.equal(h.state.tagCommit, h.sha);
  assert.equal(h.state.release.draft, false);
  assert.equal(h.state.release.make_latest, 'legacy');
  assert.equal(h.calls.at(-2).args[1], 'upload');
  assert.equal(h.calls.at(-1).body.draft, false);
  assert.equal(h.run({ sha: 'c'.repeat(40) }), 'unchanged');
  assert.equal(h.state.tagCommit, h.sha, 'later commits of the same version must not retag');
});

test('published release is a read-only no-op even from a newer main commit', t => {
  const h = harness(t, { release: 'published', tagCommit: 'c'.repeat(40) });
  assert.equal(h.run(), 'unchanged');
  assert.equal(h.calls.length, 1);
});

test('manual version-tag runs support existing annotated tags', t => {
  const h = harness(t, { tagCommit: 'a'.repeat(40), annotated: true });
  assert.equal(h.run({ ref: 'refs/tags/v0.8.1', event: 'workflow_dispatch' }), 'published');
  assert.ok(!h.calls.some(call => call.args[1].endsWith('/git/refs')));
});

test('upload failure leaves a recoverable draft; retry completes it without retagging', t => {
  const h = harness(t, { uploadFailure: true });
  assert.throws(() => h.run(), /HTTP 503/);
  assert.equal(h.state.release.draft, true);
  const before = h.calls.length;
  h.state.uploadFailure = false;
  assert.equal(h.run({ event: 'workflow_dispatch' }), 'published');
  assert.ok(!h.calls.slice(before).some(call => call.args.includes('POST')));
});

test('an unpublished tag at another commit cannot be overwritten or published', t => {
  const h = harness(t, { tagCommit: 'c'.repeat(40), release: 'draft' });
  assert.throws(() => h.run(), /points to another commit/);
  assert.ok(!h.calls.some(call => call.args.includes('--method') || call.args[0] === 'release'));
});

test('missing packages fail before creating a tag or draft', t => {
  const h = harness(t, { incomplete: true });
  assert.throws(() => h.run(), /every supported platform/);
  assert.equal(h.state.tagCommit, null);
  assert.equal(h.state.release, null);
});

test('prereleases never become latest', t => {
  const h = harness(t, { version: '0.9.0-beta.1' });
  assert.equal(h.run(), 'published');
  assert.equal(h.state.release.prerelease, true);
  assert.equal(h.state.release.make_latest, 'false');
});

test('authentication/server errors are not mistaken for missing releases', t => {
  for (const apiFailure of [403, 500]) {
    const h = harness(t, { apiFailure });
    assert.throws(() => h.run(), new RegExp(`HTTP ${apiFailure}`));
    assert.equal(h.calls.length, 1);
  }
});

test('PRs, other branches, and mismatched tags cannot publish', t => {
  for (const overrides of [{ event: 'pull_request' }, { ref: 'refs/heads/feature' }, { ref: 'refs/tags/v99.0.0' }]) {
    const h = harness(t);
    assert.throws(() => h.run(overrides), /Publishing is allowed only|must match/);
    assert.equal(h.calls.length, 0);
  }
});
