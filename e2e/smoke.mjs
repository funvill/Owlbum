// The smoke test (issue 204): drives the real program in its real WebView2
// window through tauri-driver and checks the items of the owner's
// checklist (issue 69) that WebDriver can reach. Started by
// scripts/smoke.sh, which builds the program, makes the album and the temp
// data folder, and starts the driver.
//
// Usage: node e2e/smoke.mjs <owlbum.exe> <album folder> <data folder>
//
// Every check runs even when an earlier one failed; the exit code is 1 when
// any failed.

import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { KEY, Session, until } from "./webdriver.mjs";

const [, , exe, album, data] = process.argv;
if (!exe || !album || !data) {
  console.error("usage: node e2e/smoke.mjs <owlbum.exe> <album folder> <data folder>");
  process.exit(2);
}

const results = [];
let before = async () => {};
async function check(name, body) {
  try {
    await before();
    const note = await body();
    results.push({ name, ok: true, note: note ?? "" });
    console.log(`ok    ${name}${note ? ` (${note})` : ""}`);
  } catch (e) {
    results.push({ name, ok: false, note: e.message });
    console.log(`FAIL  ${name}: ${e.message}`);
  }
}
function expect(cond, message) {
  if (!cond) throw new Error(message);
}
const pause = (ms) => new Promise((r) => setTimeout(r, ms));

/** The native window's title, read from Windows (WebDriver sees only the page's). */
function windowTitle() {
  const ps = `(Get-Process | Where-Object { $_.Path -eq '${exe.replace(/'/g, "''")}' -and $_.MainWindowTitle } | Select-Object -First 1).MainWindowTitle`;
  return execFileSync("powershell", ["-NoProfile", "-Command", ps], { encoding: "utf8" }).trim();
}
function running() {
  const ps = `@(Get-Process | Where-Object { $_.Path -eq '${exe.replace(/'/g, "''")}' }).Count`;
  return Number(execFileSync("powershell", ["-NoProfile", "-Command", ps], { encoding: "utf8" }).trim()) > 0;
}
function log() {
  const file = join(data, "logs", "owlbum.log");
  return existsSync(file) ? readFileSync(file, "utf8") : "";
}

const visible = (selector) =>
  `(() => { const e = document.querySelector(${JSON.stringify(selector)}); return e !== null && e.getClientRects().length > 0; })()`;

const s = await Session.start("http://127.0.0.1:4444", {
  browserName: "wry",
  "tauri:options": { application: exe },
});
const is = (selector) => s.run(`function () { return ${visible(selector)}; }`);
const text = (selector) => s.run((sel) => document.querySelector(sel)?.innerText ?? "", selector);
const escape = () => s.press(KEY.escape);
const layered = () => s.run(() => document.querySelector('[aria-modal="true"], [role="menu"]') !== null);
/** Closes whatever a failed check left open, so the next check starts clean. */
async function settle() {
  for (let i = 0; i < 4 && (await layered()); i++) {
    await escape();
    await pause(300);
  }
}

let version = "";
try {
  await until("the start screen", () => s.run(() => /Open folder/.test(document.body.innerText)), 30000);
  // Records CSP violations from here on; the browser log covers the start.
  await s.run(() => {
    window.__smokeCsp = [];
    document.addEventListener("securitypolicyviolation", (e) =>
      window.__smokeCsp.push(`${e.violatedDirective} ${e.blockedURI}`),
    );
  });

  before = () => settle();
  await check("Window title before an album opens: Owlbum <version>", async () => {
    const title = await until("the window title", () => windowTitle());
    const m = /^Owlbum (\d+\.\d+\.\d+\S*)$/.exec(title);
    expect(m, `title is "${title}"`);
    version = m[1];
    return title;
  });

  await check("F1 opens About (version, data folder, credits)", async () => {
    await s.press(KEY.f1);
    await until("About", () => is(".ow-about"));
    const about = await text(".ow-about");
    expect(about.includes(version), `About does not show ${version}`);
    expect(about.includes(data), `About does not show the data folder ${data}`);
    expect(/It includes software from other projects/.test(about), "the credits line is missing");
    await escape();
    await until("About to close", async () => !(await is(".ow-about")));
  });

  await check("F2 Give feedback is handled (browser stubbed)", async () => {
    await s.press(KEY.f2);
    const line = await until("the stubbed browser's log line", () =>
      log()
        .split("\n")
        .find((l) => l.includes("not opened (OWLBUM_NO_BROWSER)")),
    );
    return line.slice(line.indexOf("https://"));
  });

  await check("Ctrl+O opens a folder; the title gains the folder", async () => {
    await s.press([KEY.control, "o"]);
    await until("tiles", async () => (await s.run(() => document.querySelectorAll(".ow-tile").length)) > 0, 30000);
    const want = `Owlbum ${version} - ${album}`;
    const title = await until("the title with the folder", async () => {
      const t = windowTitle();
      return t === want ? t : false;
    });
    return title;
  });

  await check("Thumbnails load (naturalWidth > 0)", async () => {
    const counts = await until(
      "the thumbnails to finish",
      async () => {
        const c = await s.run(() => {
          const imgs = [...document.querySelectorAll("img.ow-tile__picture")];
          return {
            all: imgs.length,
            loaded: imgs.filter((i) => i.complete && i.naturalWidth > 0).length,
            pending: imgs.filter((i) => !i.complete).length,
          };
        });
        return c.all > 0 && c.pending === 0 ? c : false;
      },
      30000,
    );
    expect(counts.loaded === counts.all, `${counts.loaded} of ${counts.all} drew`);
    return `${counts.loaded} of ${counts.all} drew`;
  });

  await check("The preview image loads (naturalWidth > 0)", async () => {
    const w = await until(
      "the preview",
      () => s.run(() => [...document.querySelectorAll(".ow-preview img")].find((i) => i.complete && i.naturalWidth > 0)?.naturalWidth ?? 0),
      30000,
    );
    return `${w} px wide`;
  });

  await check("F stars the current photo", async () => {
    const starred = await s.run(() => document.querySelectorAll(".ow-tile__star--on").length);
    await s.press("f");
    await until("a star", async () => (await s.run(() => document.querySelectorAll(".ow-tile__star--on").length)) > starred);
  });

  await check("F1 works while typing in the search box", async () => {
    await s.run(() => document.querySelector('input[type="search"]')?.focus());
    await s.press(KEY.f1);
    await until("About", () => is(".ow-about"));
    await escape();
    await until("About to close", async () => !(await is(".ow-about")));
    await s.run(() => document.activeElement?.blur());
  });

  await check("? opens the shortcut overlay, and its Settings… button opens Settings", async () => {
    await s.press("?");
    await until("the overlay", () => is(".ow-shortcuts"));
    const overlay = await text(".ow-shortcuts");
    expect(/F1/.test(overlay) && /Ctrl\s*O/.test(overlay), "the overlay does not list F1 and Ctrl+O");
    await s.click(() =>
      [...document.querySelector(".ow-shortcuts").closest('[aria-modal="true"]').querySelectorAll("button")].find((x) =>
        /Settings/.test(x.innerText),
      ),
    );
    await until("Settings", () => is(".ow-settings"));
  });

  // docs/08 "Typing": while a dialog is open the page ignores the key
  // rows, F1 included (the checklist on issue 69 predates that rule).
  await check("Inside a dialog (Settings) F1 is ignored, as docs/08 says", async () => {
    await s.click(() => document.querySelector('.ow-topbar button[aria-label="Settings"]'));
    await until("Settings", () => is(".ow-settings"));
    await s.press(KEY.f1);
    await pause(1500);
    expect(!(await is(".ow-about")), "About opened over Settings");
    await escape();
    await until("Settings to close", async () => !(await is(".ow-settings")));
  });

  await check("F11 enters and leaves full screen", async () => {
    const full = () => s.run(() => window.innerWidth === screen.width && window.innerHeight === screen.height);
    expect(!(await full()), "the window starts full screen");
    await s.press(KEY.f11);
    await until("full screen", full);
    await s.press(KEY.f11);
    await until("the window again", async () => !(await full()));
  });

  await check("T hides and shows the thumbnails", async () => {
    // The details panel keeps a few tiles of its own (a stack's photos).
    const shown = () => s.run(() => document.querySelector('button[aria-label="Thumbnails"]').classList.contains("ow-icon-button--on"));
    const tiles = () => s.run(() => document.querySelectorAll(".ow-grid .ow-tile, .ow-tile").length);
    const all = await tiles();
    expect(await shown(), "the thumbnails are not shown to begin with");
    await s.press("t");
    await until("the thumbnails to hide", async () => !(await shown()) && (await tiles()) < all);
    await s.press("t");
    await until("the thumbnails to come back", async () => (await shown()) && (await tiles()) === all);
  });

  await check("I hides and shows the details panel", async () => {
    await s.press("i");
    await until("the details panel to hide", async () => !(await is(".ow-details")));
    await s.press("i");
    await until("the details panel to come back", () => is(".ow-details"));
  });

  await check("The More menu opens", async () => {
    await s.click(() => document.querySelector('button[aria-label="More"]'));
    const items = await until("the menu", () =>
      s.run(() => [...document.querySelectorAll('[role="menu"] [role^="menuitem"]')].map((i) => i.innerText.trim()).filter(Boolean)),
    );
    expect(items.length > 0, "the menu has no items");
    await until("the menu to take focus", () => s.run(() => document.activeElement?.closest('[role="menu"]') != null));
    await escape();
    await until("the menu to close", async () => (await s.run(() => document.querySelector('[role="menu"]'))) === null);
    return `${items.length} items`;
  });

  await check("Map asks before loading tiles (no network)", async () => {
    await s.click(() => [...document.querySelectorAll(".ow-segmented__segment")].find((b) => b.innerText === "Map"));
    await until("the consent card", () => s.run(() => /Load maps/.test(document.body.innerText)));
    const tiles = await s.run(() => document.querySelectorAll('img[src*="tile.openstreetmap.org"]').length);
    expect(tiles === 0, `${tiles} map tiles were requested`);
    await s.click(() => [...document.querySelectorAll(".ow-segmented__segment")].find((b) => b.innerText === "Grid"));
  });

  await check("Ctrl+W closes the folder; the title goes back", async () => {
    await s.press([KEY.control, "w"]);
    const want = `Owlbum ${version}`;
    await until("the start screen", () => s.run(() => document.querySelectorAll(".ow-tile").length === 0));
    return until("the title without the folder", async () => {
      const t = windowTitle();
      return t === want ? t : false;
    });
  });

  await check("No CSP errors (page listener and browser log)", async () => {
    const seen = await s.run(() => window.__smokeCsp);
    const browser = (await s.browserLog()) ?? [];
    const csp = browser.filter((m) => /Content Security Policy|Refused to/i.test(m.message));
    expect(seen.length === 0 && csp.length === 0, [...seen, ...csp.map((m) => m.message)].join("; "));
    const severe = browser.filter((m) => m.level === "SEVERE").map((m) => m.message);
    return severe.length ? `other errors in the log: ${severe.join(" | ")}` : "log clean";
  });

  await check("Ctrl+Q quits cleanly", async () => {
    await s.press([KEY.control, "q"]).catch(() => {});
    await until("the program to exit", () => !running(), 15000);
    const panics = log()
      .split("\n")
      .filter((l) => /panick|ERROR/.test(l));
    expect(panics.length === 0, panics.join(" | "));
  });
} finally {
  await s.end();
}

const failed = results.filter((r) => !r.ok);
if (failed.length) console.log(`\nThe end of the program's log:\n${log().split("\n").slice(-40).join("\n")}`);
console.log(`\n${results.length - failed.length} of ${results.length} checks passed.`);
process.exit(failed.length ? 1 : 0);
