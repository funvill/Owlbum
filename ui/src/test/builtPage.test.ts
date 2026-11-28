// @vitest-environment node
// The built page against the network policy (docs/08-desktop-and-release.md,
// "Network policy"): it names no remote address other than the tile host,
// and it has no inline script, so the Content-Security-Policy needs no
// script exception.

import { mkdtempSync, readdirSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { build } from "vite";
import { afterAll, beforeAll, describe, expect, it } from "vitest";

import { srcDir } from "./sources";

/**
 * Addresses the bundle may contain that are never requested: XML namespace
 * names that React and Leaflet compare against, React's error-page prefix in
 * its minified error messages, and Leaflet's attribution prefix, which the
 * map turns off.
 */
const NOT_REQUESTED = [
  "http://www.w3.org/",
  "https://react.dev/errors/",
  "https://leafletjs.com",
];

/** The only remote host the page loads anything from. */
const TILE_HOST = "https://tile.openstreetmap.org/";

/** The custom scheme and IPC in their Windows form, which are local. */
const LOCAL = /^https?:\/\/[a-z.]*\.localhost[/:]/;

/** Remote addresses in a text that the policy does not allow. */
function remoteAddresses(text: string): string[] {
  const found = text.match(/https?:\/\/[^\s"'`)<>\\]+/g) ?? [];
  return found.filter(
    (url) =>
      !url.startsWith(TILE_HOST) &&
      !LOCAL.test(url) &&
      !NOT_REQUESTED.some((prefix) => url.startsWith(prefix)),
  );
}

/** `<script>` elements in an HTML text that have no `src`. */
function inlineScripts(html: string): string[] {
  return (html.match(/<script\b[^>]*>/gi) ?? []).filter((tag) => !/\ssrc=/i.test(tag));
}

describe("the policy checks", () => {
  it("find a remote address and pass the allowed ones", () => {
    expect(remoteAddresses('a("https://example.com/x.js")')).toEqual(["https://example.com/x.js"]);
    expect(
      remoteAddresses(
        "`https://tile.openstreetmap.org/{z}/{x}/{y}.png` http://owlbum.localhost/ http://www.w3.org/2000/svg",
      ),
    ).toEqual([]);
  });

  it("find an inline script", () => {
    expect(inlineScripts('<script type="module" src="/a.js"></script>')).toEqual([]);
    expect(inlineScripts("<script>alert(1)</script>")).toHaveLength(1);
  });
});

describe("the built page", () => {
  let outDir = "";
  let files: { name: string; text: string }[] = [];

  beforeAll(async () => {
    outDir = mkdtempSync(join(tmpdir(), "owlbum-page-"));
    // Vitest sets NODE_ENV to "test", which would bundle React's development
    // build; the release is built for production.
    const nodeEnv = process.env.NODE_ENV;
    process.env.NODE_ENV = "production";
    try {
      await build({
        root: join(srcDir, ".."),
        mode: "production",
        logLevel: "silent",
        build: { outDir, emptyOutDir: true },
      });
    } finally {
      process.env.NODE_ENV = nodeEnv;
    }
    const walk = (dir: string, prefix: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = join(dir, entry.name);
        if (entry.isDirectory()) walk(path, `${prefix}${entry.name}/`);
        else if (/\.(html|js|css)$/.test(entry.name))
          files.push({ name: `${prefix}${entry.name}`, text: readFileSync(path, "utf8") });
      }
    };
    walk(outDir, "");
  }, 120_000);

  afterAll(() => {
    if (outDir) rmSync(outDir, { recursive: true, force: true });
    files = [];
  });

  it("names no remote address other than the tile host", () => {
    expect(files.some((f) => f.name.endsWith(".js"))).toBe(true);
    const found = files.flatMap((f) => remoteAddresses(f.text).map((url) => `${f.name}: ${url}`));
    expect(found).toEqual([]);
  });

  it("has no inline script", () => {
    const html = files.filter((f) => f.name.endsWith(".html"));
    expect(html.length).toBeGreaterThan(0);
    expect(html.flatMap((f) => inlineScripts(f.text))).toEqual([]);
  });
});
