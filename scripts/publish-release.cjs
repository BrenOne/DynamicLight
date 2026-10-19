const fs = require('node:fs');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const { version, assemble } = require('./release.cjs');

function publishRelease({ root, directory, repo, sha, ref, event }, execute = (args, input) =>
  spawnSync('gh', args, { input, encoding: 'utf8', maxBuffer: 16 * 1024 * 1024 })) {
  if (!['push', 'workflow_dispatch'].includes(event) || !(ref === 'refs/heads/main' || ref?.startsWith('refs/tags/v'))) {
    throw new Error('Publishing is allowed only for main or version-tag push/manual runs');
  }
  if (!/^[\w.-]+\/[\w.-]+$/.test(repo) || !/^[a-f0-9]{40}$/.test(sha)) throw new Error('Invalid repository or build commit');
  const tag = `v${version(root, ref.startsWith('refs/tags/') ? ref.slice(10) : undefined)}`;
  function gh(args, input, allowMissing = false) {
    const result = execute(args, input);
    if (result.error) throw result.error;
    if (result.status !== 0) {
      if (allowMissing && /\(HTTP 404\)/.test(result.stderr)) return null;
      throw new Error(`gh ${args.slice(0, 2).join(' ')} failed: ${result.stderr || result.stdout}`);
    }
    return result.stdout;
  }
  function api(endpoint, method, body, allowMissing = false) {
    const args = ['api', endpoint];
    if (method) args.push('--method', method, '--input', '-');
    const output = gh(args, body ? JSON.stringify(body) : undefined, allowMissing);
    return output === null ? null : JSON.parse(output);
  }
  const base = `repos/${repo}`;
  // List releases with authentication so interrupted drafts are included too.
  const releases = JSON.parse(gh(['api', `${base}/releases?per_page=100`, '--paginate', '--slurp'])).flat();
  let release = releases.find(item => item.tag_name === tag);
  if (release && !release.draft) {
    console.log(`${tag} is already published; leaving its tag and assets unchanged.`);
    return 'unchanged';
  }

  let tagObject = api(`${base}/git/ref/tags/${tag}`, undefined, undefined, true)?.object;
  const tagExists = Boolean(tagObject);
  // Resolve annotated tags as well as the lightweight tags created below.
  for (let depth = 0; tagObject?.type === 'tag' && depth < 8; depth++) {
    tagObject = api(`${base}/git/tags/${tagObject.sha}`).object;
  }
  if (tagExists && (tagObject?.type !== 'commit' || tagObject.sha !== sha)) {
    throw new Error(`${tag} points to another commit; refusing to move it or publish mismatched packages`);
  }

  // Validate the complete current-run asset set before creating any remote state.
  assemble(root, directory, tag);
  const notes = fs.readFileSync(path.join(root, 'docs/release-notes.md'), 'utf8');
  if (!tagExists) api(`${base}/git/refs`, 'POST', { ref: `refs/tags/${tag}`, sha });
  const prerelease = tag.includes('-');
  if (!release) {
    release = api(`${base}/releases`, 'POST', {
      tag_name: tag, target_commitish: sha, name: `DynamicLight ${tag}`,
      draft: true, prerelease, generate_release_notes: true, body: notes,
    });
  }
  const files = fs.readdirSync(directory).sort().map(name => path.join(directory, name));
  gh(['release', 'upload', tag, ...files, '--clobber', '--repo', repo]);
  // Legacy selection compares versions, so a delayed older build cannot become
  // latest just because it finished after a newer version's release.
  api(`${base}/releases/${release.id}`, 'PATCH', {
    draft: false, prerelease, make_latest: prerelease ? 'false' : 'legacy',
  });
  console.log(`Published ${tag} from ${sha}.`);
  return 'published';
}

module.exports = { publishRelease };
if (require.main === module) {
  publishRelease({
    root: path.resolve(__dirname, '..'), directory: process.argv[2] || 'dist/release',
    repo: process.env.GITHUB_REPOSITORY, sha: process.env.GITHUB_SHA,
    ref: process.env.GITHUB_REF, event: process.env.GITHUB_EVENT_NAME,
  });
}
