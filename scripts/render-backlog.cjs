const fs = require('node:fs');
const items = JSON.parse(fs.readFileSync('docs/backlog.json', 'utf8'));
const links = fs.existsSync('docs/github-backlog.json') ? JSON.parse(fs.readFileSync('docs/github-backlog.json', 'utf8')) : {};
const progress = JSON.parse(fs.readFileSync('docs/backlog-progress.json', 'utf8'));
let md = `# DynamicLight backlog\n\nReviewed ${progress.updated} against merged PRs #39 and #40. Partial means some acceptance criteria are implemented; it does not mean the issue is complete. Platform builds do not replace physical validation.\n\n`;
md += 'Priority: **P1** = foundations and core usability; **P2** = the next layer of product capability; **P3** = exploratory or integration work. Dependencies are recorded in the issue bodies.\n\n';
if (links.project) md += `[GitHub project](${links.project.url}) · [All issues](https://github.com/mskutin/DynamicLight/issues)\n\n`;
md += '## Agreed order\n\n1. Startup, sleep, lock and reconnect (#1, #2), with actionable recovery states.\n2. Saved profiles and tray controls (#5, #33).\n3. Guided strip calibration and screen sampling (#4, #3).\n4. Finish beat detection, then screen/music blending (#6, #20).\n\nCustom editors, plugins, phone control and game integrations remain deferred. Physical Mac/Linux validation remains a release requirement.\n\n## Implemented portions and remaining work\n\n';
for (const [key, value] of Object.entries(progress.items)) {
  const issue = links.issues[key];
  md += `- **[${key} · #${issue.number}](${issue.url}) — ${value.status}.** ${value.delivered} Remaining: ${value.remaining}\n`;
}
md += '\n';
for (const phase of ['Foundation', 'Studio', 'Ecosystem']) {
  md += `## ${phase}\n\n| ID | Feature | Area | Priority | Issue |\n| --- | --- | --- | --- | --- |\n`;
  for (const item of items.filter(item => item.phase === phase)) {
    const issue = links.issues?.[item.key];
    md += `| ${item.key} | ${item.title} | ${item.area} | ${item.priority} | ${issue ? `[#${issue.number}](${issue.url})` : 'Pending publication'} |\n`;
  }
  md += '\n';
}
md += 'The original planning baseline is [docs/backlog.json](docs/backlog.json); current implementation notes and sequencing are in [docs/backlog-progress.json](docs/backlog-progress.json). Each issue retains its acceptance checklist. Do not close partial issues until the remaining work and physical checks are complete.\n';
fs.writeFileSync('BACKLOG.md', md);
console.log(`Rendered ${items.length} backlog items.`);
