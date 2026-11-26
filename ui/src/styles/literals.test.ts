// docs/09-visual-design.md: "No component may write a colour, a z-index or a
// duration as a literal. A page test fails if a style outside the token file
// contains a hex colour, `rgb(` or `hsl(`." And from "Focus": no style sets
// `outline: none` without drawing the ring in its place.

import { describe, expect, it } from "vitest";
import { join } from "node:path";
import { read, sourceFiles, srcDir } from "../test/sources";

const checked = sourceFiles([".css", ".ts", ".tsx"]).filter(
  (file) =>
    file !== "styles/tokens.css" &&
    !file.startsWith("api/bindings/") &&
    !file.startsWith("test/") &&
    !/\.test\.tsx?$/.test(file),
);
const cssFiles = checked.filter((file) => file.endsWith(".css"));

/** Each `line: text` in a file that matches. */
function hits(files: string[], pattern: RegExp): string[] {
  const found: string[] = [];
  for (const file of files) {
    read(join(srcDir, file))
      .split("\n")
      .forEach((line, i) => {
        if (pattern.test(line)) found.push(`${file}:${i + 1}: ${line.trim()}`);
      });
  }
  return found;
}

/** Each CSS declaration of a property that matches `property`, comments removed, as one line. */
function cssDeclarations(property: RegExp): string[] {
  const found: string[] = [];
  for (const file of cssFiles) {
    const css = read(join(srcDir, file)).replace(/\/\*[\s\S]*?\*\//g, "");
    for (const match of css.matchAll(/([a-z-]+)\s*:([^;{}]*);/g)) {
      if (property.test(match[1]!)) found.push(`${file}: ${match[0].replace(/\s+/g, " ")}`);
    }
  }
  return found;
}

const tsFiles = checked.filter((file) => !file.endsWith(".css"));

describe("style literals", () => {
  it("checks the page's styles and components", () => {
    expect(cssFiles.length).toBeGreaterThan(5);
    expect(checked.length).toBeGreaterThan(cssFiles.length);
  });

  it("writes no colour outside the token file", () => {
    const colour = /#[0-9a-fA-F]{3,8}\b|\brgba?\(|\bhsla?\(/;
    expect(hits(cssFiles, colour)).toEqual([]);
    // In code, only inside a string, so a comment such as "see #123" passes.
    expect(hits(tsFiles, /["'`][^"'`]*(#[0-9a-fA-F]{3,8}\b|\brgba?\(|\bhsla?\()/)).toEqual([]);
  });

  it("writes no z-index as a number", () => {
    expect(hits(checked, /z-?index\s*:\s*-?\d/i)).toEqual([]);
  });

  it("writes no duration as a number in a transition or animation", () => {
    const timed = cssDeclarations(/^(transition|animation)/);
    expect(timed.length).toBeGreaterThan(5);
    expect(timed.filter((d) => /:.*\b\d+(\.\d+)?m?s\b/.test(d))).toEqual([]);
  });

  it("never removes the focus outline", () => {
    expect(hits(checked, /outline\s*:\s*(none|0)\b/)).toEqual([]);
  });
});
