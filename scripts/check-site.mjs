// Checks the static website in site/ (docs/08-desktop-and-release.md,
// "The marketing site"): every local link and file it names exists, every
// #anchor names an id on the page, it has no script, and it loads nothing
// from another host. Links to other sites are allowed only for anchors, and
// only to the project on GitHub. No network is used.
import { existsSync, readFileSync } from "node:fs";
import { dirname, join, normalize } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "site");
const problems = [];
const fail = (msg) => problems.push(msg);

const html = readFileSync(join(root, "index.html"), "utf8");
const css = readFileSync(join(root, "style.css"), "utf8");

if (/<script\b/i.test(html)) fail("index.html has a <script>; the site has no script.");
if (/<(iframe|object|embed)\b/i.test(html)) fail("index.html embeds another page or object.");
if (/\son[a-z]+\s*=/i.test(html)) fail("index.html has an inline event handler.");

const ids = new Set([...html.matchAll(/\sid="([^"]+)"/g)].map((m) => m[1]));
const allowedHost = "https://github.com/funvill/Owlbum";

function checkLocal(ref, where) {
  const path = ref.split("#")[0].split("?")[0];
  if (path === "") return;
  const full = normalize(join(root, decodeURI(path)));
  if (!full.startsWith(normalize(root))) fail(`${where}: ${ref} points outside site/.`);
  else if (!existsSync(full)) fail(`${where}: ${ref} does not exist.`);
}

// Every tag with src= or href=.
for (const m of html.matchAll(/<([a-z]+)\b[^>]*?\s(src|href)="([^"]*)"[^>]*>/gi)) {
  const [tag, attr, ref] = [m[1].toLowerCase(), m[2].toLowerCase(), m[3]];
  const where = `index.html <${tag} ${attr}>`;
  if (ref === "") fail(`${where} is empty.`);
  else if (ref.startsWith("#")) {
    if (ref.length > 1 && !ids.has(ref.slice(1))) fail(`${where}: no element has id "${ref.slice(1)}".`);
  } else if (/^[a-z]+:|^\/\//i.test(ref)) {
    if (tag !== "a") fail(`${where}: ${ref} would load something from another host.`);
    else if (ref !== allowedHost && !ref.startsWith(allowedHost + "/"))
      fail(`${where}: ${ref} is not a link to the project on GitHub.`);
  } else checkLocal(ref, where);
}

for (const m of css.matchAll(/url\(\s*["']?([^"')]+)["']?\s*\)/g)) {
  const ref = m[1];
  if (/^[a-z]+:|^\/\//i.test(ref)) fail(`style.css: url(${ref}) would load something from another host.`);
  else checkLocal(ref, "style.css url()");
}
if (/@import\b/i.test(css)) fail("style.css uses @import.");

if (problems.length > 0) {
  console.error(`The website has problems:\n  ${problems.join("\n  ")}`);
  process.exit(1);
}
console.log("site ok");
