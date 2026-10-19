// Exercise the publisher without network access or writes to the checkout.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const source = fs.readFileSync('scripts/publish-backlog.cjs', 'utf8');
const project = { id: 'project', number: 4, title: 'DynamicLight Roadmap', url: 'https://example.test/project' };
const item = key => ({ key, title: key, summary: key, criteria: ['Verify behavior'], area: 'ui', priority: 'P2', phase: 'Studio' });
function run({ items, issues, mappings = {}, members = [], progress = {}, pending = {} }) {
  const files = new Map(Object.entries({
    'docs/backlog.json': JSON.stringify(items),
    'docs/backlog-progress.json': JSON.stringify({ items: progress }),
    'docs/github-backlog.json': JSON.stringify({ project, issues: mappings, pending })
  }));
  const writes = [];
  const fields = [
    { id: 'status', name: 'Status', options: [{ id: 'backlog', name: 'Backlog' }] },
    { id: 'priority', name: 'Priority', options: [{ id: 'p2', name: 'P2' }] },
    { id: 'phase', name: 'Phase', options: [{ id: 'studio', name: 'Studio' }] }
  ];
  const execFileSync = (_, args) => {
    const command = args.slice(0, 2).join(' ');
    if (command === 'api user') return JSON.stringify({ login: 'mskutin' });
    if (args[0] === 'api' && args[1].includes('/labels?')) return JSON.stringify(['enhancement', 'bug', 'priority:p1', 'priority:p2', 'priority:p3', 'area:ui', 'phase:foundation', 'phase:studio', 'phase:ecosystem'].map(name => ({ name })));
    if (command === 'repo view') return '{}';
    if (command === 'project list') return JSON.stringify({ projects: [project] });
    if (command === 'project view') return JSON.stringify(project);
    if (command === 'project field-list') return JSON.stringify({ fields });
    if (command === 'project item-list') return JSON.stringify({ items: members });
    if (command === 'issue list') return JSON.stringify(issues);
    const payload = args.includes('--input') ? JSON.parse(files.get(args[args.indexOf('--input') + 1])) : undefined;
    writes.push({ args, payload });
    if (command === 'project link') return '';
    if (command === 'project item-add') return JSON.stringify({ id: 'new-member' });
    if (command === 'api graphql') return JSON.stringify({ data: {} });
    if (args[0] === 'api' && args[1].endsWith('/issues')) return JSON.stringify({ number: 100 + writes.length, html_url: `https://example.test/issue/${writes.length}` });
    if (args[0] === 'api' && /\/issues\/\d+$/.test(args[1])) return '{}';
    throw new Error(`Unexpected command: ${args.join(' ')}`);
  };
  const context = { require: name => name === 'node:fs' ? {
    readFileSync: p => files.get(p), existsSync: p => files.has(p), mkdirSync: () => {}, writeFileSync: (p, value) => files.set(p, value)
  } : name === 'node:child_process' ? { execFileSync } : require(name), console: { log: () => {} } };
  vm.runInNewContext(source, context);
  return writes.filter(w => w.args.slice(0, 2).join(' ') !== 'project link');
}
const existing = [{ number: 1, url: 'https://example.test/1', body: 'Hand-edited, closed work <!-- backlog-key: OLD -->' }, { number: 2, url: 'https://example.test/2', body: 'Intentionally excluded from project' }];
assert.deepEqual(run({ items: [item('OLD'), item('EXCLUDED')], issues: existing, mappings: { EXCLUDED: { number: 2, url: existing[1].url } }, members: [{ id: 'existing', content: { url: existing[0].url }, status: 'Done' }] }), []);
assert.throws(() => run({ items: [item('MISSING')], issues: [], mappings: { MISSING: { number: 99 } } }), /Cannot recreate tracked work/);
assert.throws(() => run({ items: [item('OLD')], issues: [...existing, existing[0]] }), /Duplicate issues/);
const writes = run({ items: [{ ...item('NEW'), dependsOn: ['LATER'] }, item('LATER')], issues: [] });
assert.equal(writes.filter(w => w.args[1].endsWith('/issues')).length, 2);
const resolved = writes.find(w => w.args.includes('PATCH'));
assert.match(resolved.payload.body, /Depends on unfinished work: #\d+/);
assert.equal(writes.filter(w => w.args.slice(0, 2).join(' ') === 'project item-add').length, 2);
assert.equal(writes.filter(w => w.args[1] === 'graphql').length, 2);
const resumed = run({ items:[item('OLD')],issues:existing,mappings:{OLD:{number:1,url:existing[0].url}},pending:{OLD:true} });
assert.equal(resumed.filter(w=>w.args.slice(0,2).join(' ')==='project item-add').length,1,'An interrupted publication resumes missing membership');
const partial = run({ items:[item('OLD')],issues:existing,mappings:{OLD:{number:1,url:existing[0].url}},pending:{OLD:true},members:[{id:'member',content:{url:existing[0].url},status:'In Progress',priority:'P1'}] });
assert.equal(partial.filter(w=>w.args.slice(0,2).join(' ')==='project item-add').length,0);
assert.equal(partial.find(w=>w.args[1]==='graphql').payload.variables.value0,'studio','Resume initializes only the missing field');
console.log('Backlog publisher: existing triage preserved; missing/duplicate identities rejected; new dependencies and project items published.');
