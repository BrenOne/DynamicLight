// PostToolUse hook: formats an edited .rs file in place, so the agent never
// spends a turn on `cargo fmt --check` failures. Silent on success; a rustfmt
// error (usually a syntax error) is reported back to the agent.
const { spawnSync } = require("node:child_process");

let input = "";
process.stdin.on("data", (chunk) => (input += chunk));
process.stdin.on("end", () => {
  let file;
  try {
    file = JSON.parse(input).tool_input?.file_path;
  } catch {
    return;
  }
  if (!file || !file.endsWith(".rs")) return;
  const result = spawnSync("rustfmt", ["--edition", "2021", file], {
    encoding: "utf8",
  });
  if (result.error) return; // rustfmt not installed: nothing to do
  if (result.status !== 0) {
    process.stderr.write(`rustfmt failed for ${file}:\n${result.stderr.slice(0, 2000)}`);
    process.exit(2);
  }
});
