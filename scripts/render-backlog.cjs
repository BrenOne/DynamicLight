const fs = require('node:fs');
const items = JSON.parse(fs.readFileSync('docs/backlog.json', 'utf8'));
const links = JSON.parse(fs.readFileSync('docs/github-backlog.json', 'utf8'));
const progress = JSON.parse(fs.readFileSync('docs/backlog-progress.json', 'utf8'));
const status = item => progress.items[item.key]?.status || 'Backlog';
const issueLink = key => {
  const issue = links.issues[key];
  if (!issue) throw new Error(`Missing issue mapping: ${key}`);
  return `[#${issue.number}](${issue.url})`;
};
if (new Set(items.map(item => item.key)).size !== items.length) throw new Error('Duplicate backlog key');
for (const [key, value] of Object.entries(progress.items)) {
  if (!items.some(item => item.key === key)) throw new Error(`Unknown progress key: ${key}`);
  if (!['Implemented', 'Validation'].includes(value.status)) throw new Error(`Unknown status: ${value.status}`);
  for (const target of value.validation || []) {
    if (progress.items[target]?.status !== 'Validation') throw new Error(`Invalid validation handoff: ${key}/${target}`);
  }
}
const delivered = items.filter(item => status(item) === 'Implemented');
const validation = items.filter(item => status(item) === 'Validation');
const backlog = items.filter(item => status(item) === 'Backlog');
let md = `# DynamicLight backlog\n\nReviewed ${progress.updated}: **${backlog.length} future features**, **${validation.length} validation trackers**, **${delivered.length} delivered features**. No implementation batch is currently active.\n\n[GitHub project](${links.project.url}) · [Open issues](https://github.com/mskutin/DynamicLight/issues)\n\n`;
md += 'Backlog means uncommitted future work. In Progress is reserved for active implementation. Validation tracks remaining checks and fixes; Done records delivered implementation with verification handed off explicitly. CI builds do not establish physical hardware or accessibility results.\n\n';
md += '## Validation\n\nPhysical testing remains deferred. Record results in these issues; do not mark unchecked scenarios passed based on builds or simulations.\n\n';
for (const item of validation) md += `- **${issueLink(item.key)} — ${item.title}.** ${progress.items[item.key].remaining}\n`;
md += '\n## Future work\n\nP2 expands product capability; P3 covers exploratory features and integrations. P1 is currently reserved for outstanding validation. Phase groups are not release dates. Dependencies below refer only to unfinished work; shipped foundations are linked in each issue.\n\n';
for (const phase of ['Foundation', 'Studio', 'Ecosystem']) {
  const group = backlog.filter(item => item.phase === phase);
  if (!group.length) continue;
  md += `### ${phase}\n\n| Issue | Feature | Priority | Depends on |\n| --- | --- | --- | --- |\n`;
  for (const item of group) md += `| ${issueLink(item.key)} | ${item.title} | ${item.priority} | ${(item.dependsOn || []).map(issueLink).join(', ') || '—'} |\n`;
  md += '\n';
}
md += '## Delivered\n\nThese implementation issues are closed. Their outstanding physical checks are tracked separately above.\n\n| Issue | Delivered work | Validation |\n| --- | --- | --- |\n';
for (const item of delivered) md += `| ${issueLink(item.key)} | ${item.title} | ${progress.items[item.key].validation.map(issueLink).join(', ')} |\n`;
md += '\nSources: [scope and dependencies](docs/backlog.json), [delivery and validation notes](docs/backlog-progress.json), [GitHub mapping](docs/github-backlog.json). Regenerate with `node scripts/render-backlog.cjs`. GitHub is authoritative for live status; update these snapshots when reconciling the roadmap.\n';
fs.writeFileSync('BACKLOG.md', md);
console.log(`Rendered ${backlog.length} backlog, ${validation.length} validation and ${delivered.length} delivered items.`);
