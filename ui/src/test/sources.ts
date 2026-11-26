// Helpers for tests that read the page's own source files and the design
// documents, rather than rendering anything.

import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative, sep } from "node:path";

/** The `ui/src` folder. */
export const srcDir = join(import.meta.dirname, "..");

/** The repository root. */
export const repoDir = join(srcDir, "..", "..");

/** The one file that may hold colour, z-index and duration values. */
export const tokenFile = join(srcDir, "styles", "tokens.css");

/** Reads a file as UTF-8. */
export function read(path: string): string {
  return readFileSync(path, "utf8");
}

/** Reads a design document from `docs/`. */
export function readDoc(name: string): string {
  return read(join(repoDir, "docs", name));
}

/** Every file under `ui/src` with one of the extensions, as paths relative to it with `/`. */
export function sourceFiles(extensions: string[]): string[] {
  const found: string[] = [];
  const walk = (dir: string) => {
    for (const name of readdirSync(dir)) {
      const path = join(dir, name);
      if (statSync(path).isDirectory()) walk(path);
      else if (extensions.some((ext) => name.endsWith(ext))) found.push(path);
    }
  };
  walk(srcDir);
  return found.map((path) => relative(srcDir, path).split(sep).join("/"));
}

/** The rows of every markdown table whose first cell is one backticked custom property. */
export function tokenRows(markdown: string): string[][] {
  return markdown
    .split("\n")
    .filter((line) => line.startsWith("| `--"))
    .map((line) =>
      line
        .slice(1, -1)
        .split(" | ")
        .map((cell) => cell.trim()),
    )
    .filter((row) => /^`--[a-z0-9-]+`$/.test(row[0]!));
}

/** The custom properties declared in a CSS block, in order: name to value. */
export function declarations(css: string): Map<string, string> {
  const found = new Map<string, string>();
  for (const match of css.matchAll(/(--[a-z0-9-]+)\s*:\s*([^;]+);/g)) {
    found.set(match[1]!, match[2]!.replace(/\s+/g, " ").trim());
  }
  return found;
}

/** The text of the first `{ … }` block in a CSS file. */
export function firstBlock(css: string): string {
  const start = css.indexOf("{");
  const end = css.indexOf("}", start);
  return css.slice(start + 1, end);
}
