const fs = require('node:fs');
const items = JSON.parse(fs.readFileSync('docs/backlog.json', 'utf8'));
const links = fs.existsSync('docs/github-backlog.json') ? JSON.parse(fs.readFileSync('docs/github-backlog.json', 'utf8')) : {};
let md = '# DynamicLight backlog\n\nAll feature and UI ideas discussed during initial planning are captured here, plus the macOS release-validation work. These are planned improvements, not claims about current functionality or committed release dates.\n\n';
md += 'Priority: **P1** = foundations and core usability; **P2** = the next layer of product capability; **P3** = exploratory or integration work. All issues start in **Backlog**. Dependencies are recorded in the issue bodies.\n\n';
if (links.project) md += `[GitHub project](${links.project.url}) · [All issues](https://github.com/mskutin/DynamicLight/issues)\n\n`;
for (const phase of ['Foundation', 'Studio', 'Ecosystem']) {
  md += `## ${phase}\n\n| ID | Feature | Area | Priority | Issue |\n| --- | --- | --- | --- | --- |\n`;
  for (const item of items.filter(item => item.phase === phase)) {
    const issue = links.issues?.[item.key];
    md += `| ${item.key} | ${item.title} | ${item.area} | ${item.priority} | ${issue ? `[#${issue.number}](${issue.url})` : 'Pending publication'} |\n`;
  }
  md += '\n';
}
md += 'The machine-readable source is [docs/backlog.json](docs/backlog.json). Each GitHub issue contains the user outcome and acceptance checklist. Update the issue as work progresses; the source manifest preserves the original planning baseline.\n';
fs.writeFileSync('BACKLOG.md', md);
console.log(`Rendered ${items.length} backlog items.`);
