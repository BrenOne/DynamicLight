// assets/icon.svg is the source for UI, executable, tray and bundle icons.
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { spawnSync } = require("node:child_process");

const root = path.resolve(__dirname, "..");
const check = process.argv.includes("--check");
const generated = fs.mkdtempSync(path.join(os.tmpdir(), "dynamiclight-icons-"));
// Tauri writes ICNS entries from a map, so their order varies between runs.
// Canonicalize the container order while preserving every image/mask byte.
function canonicalIcns(bytes) {
  const chunks = [];
  for (let at = 8; at < bytes.length; ) {
    const length = bytes.readUInt32BE(at + 4);
    if (length < 8 || at + length > bytes.length)
      throw Error("Invalid ICNS entry");
    chunks.push(bytes.subarray(at, at + length));
    at += length;
  }
  chunks.sort((a, b) => Buffer.compare(a.subarray(0, 4), b.subarray(0, 4)));
  return Buffer.concat([bytes.subarray(0, 8), ...chunks]);
}
try {
  const result = spawnSync(
    process.execPath,
    [
      require.resolve("@tauri-apps/cli/tauri.js"),
      "icon",
      path.join(root, "assets/icon.svg"),
      "--output",
      generated,
    ],
    { cwd: root, encoding: "utf8" },
  );
  if (result.error) throw result.error;
  if (result.status !== 0) throw Error(result.stderr || result.stdout);
  const files = [
    ["assets/icon.svg", "src/ui/icon.svg"],
    ...["icon.png", "icon.ico", "icon.icns"].map((name) => [
      path.join(generated, name),
      "src-tauri/icons/" + name,
    ]),
    [path.join(generated, "icon.png"), "assets/icon.png"],
  ];
  for (const [source, destination] of files) {
    const raw = fs.readFileSync(path.resolve(root, source));
    const bytes = destination.endsWith(".icns") ? canonicalIcns(raw) : raw;
    const target = path.join(root, destination);
    if (check) {
      if (!fs.existsSync(target) || !bytes.equals(fs.readFileSync(target)))
        throw Error(
          `${destination} is stale. Run pnpm icons and commit the generated icons.`,
        );
    } else fs.writeFileSync(target, bytes);
  }
  console.log(
    check
      ? "UI and native icons match assets/icon.svg"
      : "UI and native icons regenerated",
  );
} finally {
  if (
    path.dirname(generated) !== path.resolve(os.tmpdir()) ||
    !path.basename(generated).startsWith("dynamiclight-icons-")
  )
    throw Error("Refusing to remove an unexpected icon output directory");
  fs.rmSync(generated, { recursive: true, force: true });
}
