#!/usr/bin/env node
// Clippy warnings on the lines you touched, and nothing else.
//
// Upstream code carries hundreds of clippy warnings this fork does not own, so
// "clippy clean" is not a usable bar. This script runs cargo clippy and keeps
// only the warnings whose file:line falls inside a hunk you added or changed.
//
//   node scripts/clippy-touched.js                 # uncommitted work vs HEAD
//   node scripts/clippy-touched.js --since main    # everything since main
//   node scripts/clippy-touched.js -- -p my-crate  # override the cargo args
//
// Exit code 1 when any warning lands on a touched line.

const { execFileSync, spawnSync } = require("child_process");
const fs = require("fs");
const path = require("path");

const argv = process.argv.slice(2);
let since = "HEAD";
let cargoArgs = null;
for (let i = 0; i < argv.length; i++) {
  if (argv[i] === "--since") since = argv[++i];
  else if (argv[i] === "--") { cargoArgs = argv.slice(i + 1); break; }
}

const root = execFileSync("git", ["rev-parse", "--show-toplevel"], { encoding: "utf8" }).trim();
process.chdir(root);

if (!cargoArgs) {
  cargoArgs = fs.existsSync(path.join(root, "crates", "skate-game"))
    ? ["-p", "skate-game", "-p", "skate-mods", "-p", "skate-net", "-p", "skate-vehicles", "--locked",
       "--", "-A", "clippy::approx_constant"] // upstream skate-core trips this deny lint
    : ["--workspace", "--all-targets"];
}

// 1. Touched line ranges: added/changed hunks since `since`, plus whole untracked files.
const touched = new Map(); // file -> [[start, end], ...]
function addRange(file, start, end) {
  const key = file.replace(/\\/g, "/");
  if (!touched.has(key)) touched.set(key, []);
  touched.get(key).push([start, end]);
}
const diff = execFileSync("git", ["diff", "-U0", "--no-color", since, "--", "*.rs"], { encoding: "utf8", maxBuffer: 1 << 28 });
let current = null;
for (const line of diff.split("\n")) {
  if (line.startsWith("+++ b/")) current = line.slice(6);
  else if (line.startsWith("+++ /dev/null")) current = null;
  else if (current && line.startsWith("@@")) {
    const m = /\+(\d+)(?:,(\d+))?/.exec(line);
    if (!m) continue;
    const start = Number(m[1]);
    const count = m[2] === undefined ? 1 : Number(m[2]);
    if (count > 0) addRange(current, start, start + count - 1);
  }
}
const untracked = execFileSync("git", ["ls-files", "--others", "--exclude-standard", "--", "*.rs"], { encoding: "utf8" });
for (const file of untracked.split("\n").filter(Boolean)) addRange(file, 1, Number.MAX_SAFE_INTEGER);

if (touched.size === 0) {
  console.log(`clippy-touched: no .rs changes since ${since}.`);
  process.exit(0);
}

// 2. Run clippy in short format and keep the hits inside touched ranges.
const clippy = spawnSync("cargo", ["clippy", "--message-format=short", ...cargoArgs], {
  encoding: "utf8", maxBuffer: 1 << 28, shell: process.platform === "win32",
});
const output = (clippy.stdout || "") + (clippy.stderr || "");
const hits = [];
for (const line of output.split("\n")) {
  const m = /^(.+?\.rs):(\d+):\d+: (warning|error): (.*)$/.exec(line.trim());
  if (!m) continue;
  const file = path.relative(root, path.resolve(root, m[1])).replace(/\\/g, "/");
  const ln = Number(m[2]);
  const ranges = touched.get(file);
  if (ranges && ranges.some(([a, b]) => ln >= a && ln <= b)) hits.push(`${file}:${ln}: ${m[3]}: ${m[4]}`);
}
if (clippy.status !== 0 && !/(warning|error)/.test(output)) {
  console.error(output.trim());
  process.exit(clippy.status || 1);
}

const files = [...touched.keys()].length;
if (hits.length === 0) {
  console.log(`clippy-touched: 0 warnings on touched lines (${files} file(s) since ${since}).`);
  process.exit(0);
}
console.log(`clippy-touched: ${hits.length} warning(s) on touched lines (${files} file(s) since ${since}):`);
for (const h of [...new Set(hits)].sort()) console.log("  " + h);
process.exit(1);
