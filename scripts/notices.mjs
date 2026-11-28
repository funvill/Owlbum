// Builds resources/THIRD-PARTY-NOTICES.txt, the notices the installers ship
// and the About box opens (docs/08-desktop-and-release.md, "Licence and
// third-party notices"). The file is generated, never edited by hand and
// never committed.
//
// Usage: node scripts/notices.mjs [--strict]
//   --strict (the release workflow and scripts/build-installer.sh): any
//   missing licence text fails instead of being reported.
//
// Sources, in the order they appear in the file:
//   1. the hand-written files in notices/ (things that are neither a crate
//      nor an npm package), with ONNX Runtime's own licence and notices from
//      resources/ (scripts/fetch-ort.mjs) after its entry;
//   2. Rust crates: `cargo about generate --format json`, with about.toml
//      listing the accepted licences (a crate under any other licence fails);
//   3. npm packages: the production dependencies of ui/package.json and
//      everything they depend on, read from ui/node_modules;
//   4. the tagging models of the catalogue, downloaded only on request.

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const strict = process.argv.includes("--strict");
const problems = [];
const problem = (text) => problems.push(text);

const read = (...p) => fs.readFileSync(path.join(root, ...p), "utf8").replace(/\r\n/g, "\n").trim();
const rule = "=".repeat(78);
const thin = "-".repeat(78);

function version() {
  const toml = read("Cargo.toml");
  return /^\[workspace\.package\][\s\S]*?^version = "([^"]+)"/m.exec(toml)[1];
}

// 1. Hand-written notices.
function handWritten() {
  const out = [];
  const dir = path.join(root, "notices");
  for (const file of fs.readdirSync(dir).filter((f) => f.endsWith(".txt")).sort()) {
    out.push(read("notices", file));
    if (file === "onnxruntime.txt") {
      for (const extra of ["onnxruntime-LICENSE.txt", "onnxruntime-ThirdPartyNotices.txt"]) {
        if (fs.existsSync(path.join(root, "resources", extra))) {
          out.push(read("resources", extra));
        } else {
          problem(`resources/${extra} is missing: run node scripts/fetch-ort.mjs first.`);
        }
      }
    }
  }
  return out;
}

// 2. Rust crates, through cargo-about.
function crates() {
  let json;
  try {
    json = execFileSync(
      "cargo",
      ["about", "generate", "--format", "json", "--fail", "--locked", "--manifest-path", "src-tauri/Cargo.toml"],
      { cwd: root, encoding: "utf8", maxBuffer: 256 * 1024 * 1024, stdio: ["ignore", "pipe", "inherit"] },
    );
  } catch (e) {
    problem(
      `cargo about failed (${e.message.split("\n")[0]}). Install it with ` +
        "`cargo install cargo-about --locked --features cli`; a crate under a licence that about.toml does not accept also fails here.",
    );
    return [];
  }
  const report = JSON.parse(json);
  const out = [];
  const licences = [...report.licenses].sort((a, b) => b.used_by.length - a.used_by.length || a.id.localeCompare(b.id));
  for (const l of licences) {
    const users = l.used_by
      .map((u) => `${u.crate.name} ${u.crate.version}`)
      .sort((a, b) => a.localeCompare(b))
      .join(", ");
    if (!l.text || !l.text.trim()) problem(`no licence text for ${l.id} (used by ${users})`);
    out.push(`${l.name} (${l.id})\n\nUsed by: ${users}\n\n${(l.text || "").replace(/\r\n/g, "\n").trim()}`);
  }
  return out;
}

// 3. npm packages that ship in the page.
function npmPackages() {
  const modules = path.join(root, "ui", "node_modules");
  if (!fs.existsSync(modules)) {
    problem("ui/node_modules is missing: run npm --prefix ui ci first.");
    return [];
  }
  // Node's lookup: the nearest node_modules above the package asking.
  const resolve = (name, from) => {
    for (let dir = from; dir.startsWith(modules); dir = path.dirname(dir)) {
      const candidate = path.join(dir, "node_modules", name);
      if (fs.existsSync(path.join(candidate, "package.json"))) return candidate;
    }
    const top = path.join(modules, name);
    return fs.existsSync(path.join(top, "package.json")) ? top : null;
  };
  const seen = new Map();
  const visit = (name, from, optional) => {
    const dir = resolve(name, from);
    if (!dir) {
      if (!optional) problem(`npm package ${name} is not in ui/node_modules`);
      return;
    }
    if (seen.has(dir)) return;
    const pkg = JSON.parse(fs.readFileSync(path.join(dir, "package.json"), "utf8"));
    seen.set(dir, pkg);
    for (const dep of Object.keys(pkg.dependencies || {})) visit(dep, dir, false);
    for (const dep of Object.keys(pkg.optionalDependencies || {})) visit(dep, dir, true);
  };
  const ui = JSON.parse(read("ui", "package.json"));
  for (const dep of Object.keys(ui.dependencies || {})) visit(dep, modules, false);

  const out = [];
  const entries = [...seen.entries()].sort((a, b) => a[1].name.localeCompare(b[1].name));
  for (const [dir, pkg] of entries) {
    const licence = typeof pkg.license === "string" ? pkg.license : pkg.license?.type || "not stated";
    const file = fs.readdirSync(dir).find((f) => /^(licen[cs]e|copying)|-licen[cs]e/i.test(f));
    let text = "";
    if (file && fs.statSync(path.join(dir, file)).isFile()) {
      text = fs.readFileSync(path.join(dir, file), "utf8").replace(/\r\n/g, "\n").trim();
    } else {
      problem(`no licence file in npm package ${pkg.name} ${pkg.version} (${licence})`);
    }
    out.push(`${pkg.name} ${pkg.version} (${licence})\n\n${text}`);
  }
  return out;
}

// 4. The catalogue's models, read from the catalogue's source.
function models() {
  const src = read("crates", "owlbum-core", "src", "tagging", "catalogue.rs");
  const names = { Mit: "MIT", Apache2: "Apache-2.0", Bsd3Clause: "BSD-3-Clause" };
  const out = [];
  const re = /label: "([^"]+)",\s*repo: "([^"]+)",\s*revision: "([^"]+)",[\s\S]*?licence: Licence::(\w+),/g;
  for (const m of src.matchAll(re)) {
    out.push(`${m[1]}: https://huggingface.co/${m[2]} (revision ${m[3]}), under ${names[m[4]] || m[4]}.`);
  }
  if (!out.length) problem("no models found in crates/owlbum-core/src/tagging/catalogue.rs");
  return out;
}

const sections = [
  [
    `Third-party notices for Owlbum ${version()}`,
    "Owlbum itself is under the MIT License (LICENSE.txt). It includes software " +
      "from other projects, each under its own licence, listed below: first " +
      "the things that are not packages, then the Rust crates built into the " +
      "program (grouped by licence), then the JavaScript packages built into " +
      "its page, then the tagging models, which are not included and are " +
      "downloaded only when you ask for one.",
  ],
  ["Other software and data", ...handWritten()],
  ["Rust crates", ...crates()],
  ["JavaScript packages", ...npmPackages()],
  [
    "Downloaded only when you ask",
    "These photo-tagging models are not part of Owlbum. Settings > Photo " +
      "tagging downloads one from Hugging Face when you choose it.\n\n" +
      models().join("\n"),
  ],
];

if (problems.length) {
  for (const p of problems) console.error(`notices: ${p}`);
  if (strict) {
    console.error("notices: failed (--strict).");
    process.exit(1);
  }
}

const text = sections
  .map(([title, ...parts]) => `${rule}\n${title}\n${rule}\n\n${parts.join(`\n\n${thin}\n\n`)}\n`)
  .join("\n");
const dest = path.join(root, "resources", "THIRD-PARTY-NOTICES.txt");
fs.mkdirSync(path.dirname(dest), { recursive: true });
// CRLF so that Notepad on older Windows shows the lines.
fs.writeFileSync(dest, text.replace(/\n/g, "\r\n"));
console.log(`notices: wrote ${dest} (${problems.length} problem(s))`);
