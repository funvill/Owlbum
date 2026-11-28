// The token file against docs/09-visual-design.md: every token the document
// names exists with its dark and light values, the layers keep their order,
// and the contrast table still holds for the values in the file, in both
// themes.

import { describe, expect, it } from "vitest";
import { declarations, firstBlock, read, readDoc, sourceFiles, srcDir, tokenFile, tokenRows } from "../test/sources";
import { join } from "node:path";

const doc = readDoc("09-visual-design.md");
const css = read(tokenFile);
const dark = declarations(firstBlock(css));
const lightBlock = /:root\[data-theme="light"\] \{([^}]*)\}/.exec(css);
const light = declarations(lightBlock?.[1] ?? "");
const everyDeclared = declarations(css);

/** Whether a token value is a colour. */
const isColour = (value: string) => /^(#[0-9a-f]{6}|rgba?\()/i.test(value);

/** The CSS value the document's "Dark" or "Value" cell stands for. */
function expectedValue(row: string[]): string {
  const cell = row[1]!;
  const ticked = /^`(.+)`$/.exec(cell);
  if (ticked) return ticked[1]!;
  const type = /^(\d+) px \/ (\d+) px$/.exec(cell);
  if (type) return `${row[2]} ${type[1]}px/${type[2]}px var(--font)`;
  const px = /^(\d+) px$/.exec(cell);
  if (px) return `${px[1]}px`;
  const ms = /^(\d+) ms$/.exec(cell);
  if (ms) return `${ms[1]}ms`;
  if (/^\d+$/.test(cell)) return cell;
  throw new Error(`Cannot read the value of ${row[0]}: ${cell}`);
}

const docTokens = tokenRows(doc).map((row) => ({
  name: row[0]!.slice(1, -1),
  value: expectedValue(row).replace(/\s+/g, " "),
}));

/** The Light column of the colour tables: "same" repeats the dark value. */
const docLight = tokenRows(doc)
  .filter((row) => row.length >= 4 && (/^`.+`$/.test(row[2]!) || row[2] === "same"))
  .map((row) => ({
    name: row[0]!.slice(1, -1),
    value: (row[2] === "same" ? row[1]! : row[2]!).slice(1, -1).replace(/\s+/g, " "),
  }));

describe("design tokens", () => {
  it("finds the token tables in the design document", () => {
    expect(docTokens.length).toBeGreaterThan(90);
  });

  it.each(docTokens)("defines $name as in the document", ({ name, value }) => {
    expect(dark.get(name)).toBe(value);
  });

  it("holds the font stack, led by the bundled Inter", () => {
    expect(dark.get("--font")).toMatch(/^'Inter Variable', system-ui, sans-serif$/);
  });

  it("chooses the dark theme on the root element until the setting is read", () => {
    expect(css).toMatch(/:root\[data-theme="dark"\]/);
    expect(firstBlock(css)).toMatch(/color-scheme: dark;/);
    expect(read(join(srcDir, "..", "index.html"))).toMatch(/<html lang="en" data-theme="dark">/);
  });

  it("has a light block that sets the light colour scheme", () => {
    expect(lightBlock).not.toBeNull();
    expect(lightBlock![1]).toMatch(/color-scheme: light;/);
  });

  it("finds the Light column in the design document", () => {
    expect(docLight.length).toBeGreaterThan(50);
  });

  it.each(docLight)("defines $name for the light theme as in the document", ({ name, value }) => {
    expect(light.get(name)).toBe(value);
  });

  it("gives every colour token a light value and nothing else", () => {
    const colours = [...dark].filter(([, value]) => isColour(value)).map(([name]) => name);
    const extra = [...light.keys()].filter((name) => !dark.has(name));
    expect(colours.filter((name) => !light.has(name))).toEqual([]);
    expect(extra).toEqual([]);
  });

  it("stacks the layers in the documented order", () => {
    const order = ["base", "raised", "bar", "popover", "viewer", "dialog", "menu", "toast", "tooltip"];
    const values = order.map((name) => Number(dark.get(`--z-${name}`)));
    expect(values.every((v) => Number.isInteger(v))).toBe(true);
    expect([...values].sort((a, b) => a - b)).toEqual(values);
    expect(new Set(values).size).toBe(values.length);
  });

  it("sets every duration and travel to zero when the system asks for reduced motion", () => {
    const block = /@media \(prefers-reduced-motion: reduce\)\s*\{\s*:root\s*\{([^}]*)\}/.exec(css);
    expect(block).not.toBeNull();
    const reduced = declarations(block![1]!);
    for (const name of ["--motion-fast", "--motion-base", "--motion-slow"]) {
      expect(reduced.get(name)).toBe("0ms");
    }
    for (const name of ["--motion-travel", "--motion-press"]) {
      expect(reduced.get(name)).toBe("0px");
    }
  });

  it("uses system colours for borders and the focus ring in forced-colours mode", () => {
    const block = /@media \(forced-colors: active\)\s*\{\s*:root\s*\{([^}]*)\}/.exec(css);
    expect(block).not.toBeNull();
    const forced = declarations(block![1]!);
    expect(forced.get("--focus")).toBe("Highlight");
    expect(forced.get("--line-strong")).toBe("CanvasText");
  });

  it("defines every custom property the page uses", () => {
    const used = new Set<string>();
    for (const file of sourceFiles([".css", ".tsx", ".ts"])) {
      if (file.endsWith(".test.ts") || file.endsWith(".test.tsx")) continue;
      for (const match of read(join(srcDir, file)).matchAll(/var\((--[a-z0-9-]+)/g)) {
        used.add(match[1]!);
      }
    }
    // Custom properties a component sets for itself start with `--_`.
    const missing = [...used].filter((name) => !name.startsWith("--_") && !everyDeclared.has(name));
    expect(missing).toEqual([]);
  });
});

/** WCAG relative luminance of a `#rrggbb` colour. */
function luminance(hex: string): number {
  const channel = (i: number) => {
    const c = parseInt(hex.slice(1 + 2 * i, 3 + 2 * i), 16) / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(0) + 0.7152 * channel(1) + 0.0722 * channel(2);
}

function contrast(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x) as [number, number];
  return (hi + 0.05) / (lo + 0.05);
}

const pairs = doc
  .split("\n")
  .map((line) => /^\| `(--[a-z0-9-]+)` on `(--[a-z0-9-]+)`( \([^)]*\))? \| ([\d.]+) \| ([\d.]+) \|/.exec(line))
  .filter((m): m is RegExpExecArray => m !== null)
  .map((m) => ({
    fg: m[1]!,
    bg: m[2]!,
    // Text needs 4.5:1; a row that says "needs 3" is a border, ring or outline.
    least: m[3]?.includes("needs 3") ? 3 : 4.5,
    ratio: Number(m[4]),
    lightRatio: Number(m[5]),
  }));

describe("contrast", () => {
  it("finds the contrast table", () => {
    expect(pairs.length).toBeGreaterThan(20);
  });

  it.each(pairs)("$fg on $bg measures $ratio in the dark theme", ({ fg, bg, ratio }) => {
    const measured = contrast(dark.get(fg)!, dark.get(bg)!);
    expect(Math.abs(measured - ratio)).toBeLessThan(0.15);
  });

  it.each(pairs)("$fg on $bg measures $lightRatio in the light theme, at least $least", ({ fg, bg, lightRatio, least }) => {
    const measured = contrast(light.get(fg)!, light.get(bg)!);
    expect(Math.abs(measured - lightRatio)).toBeLessThan(0.15);
    expect(measured).toBeGreaterThanOrEqual(least);
  });
});
