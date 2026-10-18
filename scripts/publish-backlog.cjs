// Explicit maintainer operation. Requires authenticated gh with repository and project access.
// Uses stable keys and existing-item checks to avoid duplicates on retry.
const fs = require('node:fs');
const path = require('node:path');
const { execFileSync } = require('node:child_process');
const repo = 'mskutin/DynamicLight', owner = 'mskutin', title = 'DynamicLight Roadmap';
const items = JSON.parse(fs.readFileSync('docs/backlog.json', 'utf8'));
const output = 'docs/github-backlog.json';
const state = fs.existsSync(output) ? JSON.parse(fs.readFileSync(output, 'utf8')) : { issues: {} };
fs.mkdirSync('.local', { recursive: true });
const inputPath = path.resolve('.local/github-request.json');
function gh(args) { return execFileSync('gh', args, { encoding: 'utf8', maxBuffer: 8 * 1024 * 1024, windowsHide: true }).trim(); }
function json(args) { return JSON.parse(gh(args)); }
function api(endpoint, data, method = 'POST') {
  fs.writeFileSync(inputPath, JSON.stringify(data));
  return json(['api', endpoint, '--method', method, '--input', inputPath]);
}
function graphql(query, variables = {}) {
  const response = api('graphql', { query, variables });
  if (response.errors) throw new Error(JSON.stringify(response.errors));
  return response.data;
}
function save() { fs.writeFileSync(output, JSON.stringify(state, null, 2) + '\n'); }
function body(item) {
  return `## Outcome\n\n${item.summary}\n\n## Acceptance criteria\n\n${item.criteria.map(c => `- [ ] ${c}`).join('\n')}\n\n## Planning\n\n- Backlog key: ${item.key}\n- Priority: ${item.priority}\n- Phase: ${item.phase}\n- Area: ${item.area}\n${item.dependsOn?.length ? `- Depends on: ${item.dependsOn.map(key => state.issues[key] ? `#${state.issues[key].number}` : key).join(', ')}\n` : ''}\nThis is planned work; inclusion in the backlog is not a release commitment.\n\n<!-- backlog-key: ${item.key} -->`;
}
if (json(['api', 'user']).login !== owner) throw new Error('Authenticate as mskutin before publishing this backlog.');
json(['repo', 'view', repo, '--json', 'nameWithOwner']);
const labels = new Set(json(['api', `repos/${repo}/labels?per_page=100`]).map(label => label.name));
const definitions = [
  ['enhancement', 'a2eeef', 'Planned product improvement'], ['bug', 'd73a4a', 'Reproducible defect'],
  ...['P1', 'P2', 'P3'].map((p, i) => [`priority:${p.toLowerCase()}`, ['d73a4a', 'fbca04', 'c5def5'][i], `${p} planning priority`]),
  ...[...new Set(items.map(item => item.area))].map(area => [`area:${area}`, '7057ff', `${area} work`]),
  ...['Foundation', 'Studio', 'Ecosystem'].map(phase => [`phase:${phase.toLowerCase()}`, '0e8a16', `${phase} planning phase`])
];
for (const [name, color, description] of definitions) if (!labels.has(name)) api(`repos/${repo}/labels`, { name, color, description });
const projects = json(['project', 'list', '--owner', owner, '--limit', '100', '--format', 'json']).projects;
state.project = state.project || projects.find(project => project.title === title) || json(['project', 'create', '--owner', owner, '--title', title, '--format', 'json']);
save();
gh(['project', 'edit', String(state.project.number), '--owner', owner, '--visibility', 'PUBLIC', '--description', 'Feature and UI backlog for DynamicLight: reliable cross-platform USB lighting, a visual studio, and integrations.', '--readme', 'All items start in Backlog. P1 covers foundations and core usability; P2 expands the product; P3 covers exploratory integrations. Foundation, Studio, and Ecosystem are planning groups, not dated release commitments. Each issue includes acceptance criteria.']);
gh(['project', 'link', String(state.project.number), '--owner', owner, '--repo', repo]);
state.project = json(['project', 'view', String(state.project.number), '--owner', owner, '--format', 'json']); save();
let fields = json(['project', 'field-list', String(state.project.number), '--owner', owner, '--format', 'json']).fields;
for (const [name, options] of [['Priority', 'P1,P2,P3'], ['Phase', 'Foundation,Studio,Ecosystem']]) {
  if (!fields.some(field => field.name === name)) gh(['project', 'field-create', String(state.project.number), '--owner', owner, '--name', name, '--data-type', 'SINGLE_SELECT', '--single-select-options', options]);
}
fields = json(['project', 'field-list', String(state.project.number), '--owner', owner, '--format', 'json']).fields;
const statusField = fields.find(field => field.name === 'Status');
if (statusField && !statusField.options.some(option => option.name === 'Backlog')) {
  const options = statusField.options.map(option => ({ id: option.id, name: option.name === 'Todo' ? 'Backlog' : option.name, color: option.name === 'Done' ? 'GREEN' : option.name === 'In Progress' ? 'YELLOW' : 'GRAY', description: option.name === 'Todo' ? 'Planned work awaiting prioritization' : option.name }));
  graphql('mutation($input:UpdateProjectV2FieldInput!){updateProjectV2Field(input:$input){projectV2Field{... on ProjectV2SingleSelectField{id name options{id name}}}}}', { input: { fieldId: statusField.id, singleSelectOptions: options } });
  fields = json(['project', 'field-list', String(state.project.number), '--owner', owner, '--format', 'json']).fields;
}
const existingIssues = json(['issue', 'list', '--repo', repo, '--state', 'all', '--limit', '1000', '--json', 'number,title,body,url']);
for (const item of items) {
  let issue = existingIssues.find(issue => issue.body.includes(`<!-- backlog-key: ${item.key} -->`));
  if (!issue) {
    const result = api(`repos/${repo}/issues`, { title: item.title, body: body(item), labels: ['enhancement', `area:${item.area}`, `priority:${item.priority.toLowerCase()}`, `phase:${item.phase.toLowerCase()}`] });
    issue = { number: result.number, url: result.html_url };
  }
  state.issues[item.key] = { number: issue.number, url: issue.url }; save();
  console.log(`${item.key}: #${issue.number}`);
}
// Resolve forward dependencies after all issues exist.
for (const item of items.filter(item => item.dependsOn?.length)) api(`repos/${repo}/issues/${state.issues[item.key].number}`, { body: body(item) }, 'PATCH');
const projectItems = json(['project', 'item-list', String(state.project.number), '--owner', owner, '--limit', '1000', '--format', 'json']).items;
for (const item of items) {
  const issue = state.issues[item.key];
  const projectItem = projectItems.find(entry => entry.content?.url === issue.url) || json(['project', 'item-add', String(state.project.number), '--owner', owner, '--url', issue.url, '--format', 'json']);
  const assignments = [['Status', 'Backlog'], ['Priority', item.priority], ['Phase', item.phase]];
  const variables = { project: state.project.id, item: projectItem.id };
  const declarations = ['$project:ID!', '$item:ID!'];
  const mutations = assignments.map(([name, value], index) => {
    const field = fields.find(field => field.name === name);
    const option = field?.options.find(option => option.name === value);
    if (!option) throw new Error(`Missing field option: ${name}/${value}`);
    variables[`field${index}`] = field.id; variables[`value${index}`] = option.id;
    declarations.push(`$field${index}:ID!`, `$value${index}:String!`);
    return `f${index}:updateProjectV2ItemFieldValue(input:{projectId:$project,itemId:$item,fieldId:$field${index},value:{singleSelectOptionId:$value${index}}}){projectV2Item{id}}`;
  });
  graphql(`mutation(${declarations.join(',')}){${mutations.join(' ')}}`, variables);
  console.log(`Project: ${item.key}`);
}
save();
console.log(`Published ${items.length} issues to ${state.project.url}`);
