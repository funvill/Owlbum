import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import raw from "./keymap.json";
import {
  KEYMAP,
  KEY_IDS,
  KeymapError,
  handlerGaps,
  parseKeymap,
  primaryKey,
  row,
  rowsFor,
} from "./table";

describe("keymap.json", () => {
  it("loads", () => {
    expect(KEYMAP.length).toBe(raw.length);
    expect(KEYMAP.length).toBeGreaterThan(50);
  });

  it("has exactly the ids the KeyId type names, in the same order", () => {
    expect(KEYMAP.map((r) => r.id)).toEqual([...KEY_IDS]);
  });

  it("fills in false for flags a row leaves out", () => {
    expect(row("browse.hide")).toEqual({
      id: "browse.hide",
      place: "browse",
      keys: ["H"],
      hold: false,
      repeat: false,
      menu: false,
      dev: false,
      typing: false,
    });
  });

  it("marks the menu accelerators the menu bar uses, and only those", () => {
    const menu = KEYMAP.filter((r) => r.menu).map((r) => [r.id, r.keys[0], r.dev]);
    expect(menu).toEqual([
      ["app.about", "F1", false],
      ["app.feedback", "F2", false],
      ["app.open_folder", "Ctrl+O", false],
      ["app.full_screen", "F11", false],
      ["app.quit", "Ctrl+Q", false],
      ["app.close_folder", "Ctrl+W", false],
      ["app.reload", "Ctrl+R", true],
      ["app.developer_tools", "Ctrl+Shift+I", true],
    ]);
  });

  it("shows F as the star key in hints; S does the same", () => {
    expect(primaryKey("browse.star")).toBe("F");
    expect(row("viewer.star").keys).toEqual(["F", "S"]);
  });

  it("agrees row for row with the table in docs/08, \"Every key\"", () => {
    expect(docRows()).toEqual(
      KEYMAP.map((r) => ({
        id: r.id,
        place: r.place,
        keys: r.keys,
        hold: r.hold,
        repeat: r.repeat,
        menu: r.menu,
        dev: r.dev,
        typing: r.typing,
      })),
    );
  });
});

describe("rowsFor", () => {
  it("gives a place's own rows and the everywhere rows", () => {
    const ids = rowsFor("edit").map((r) => r.id);
    expect(ids).toContain("edit.done");
    expect(ids).toContain("app.shortcuts");
    expect(ids).not.toContain("browse.star");
    expect(ids).not.toContain("viewer.star");
  });

  it("leaves out development-only rows unless asked", () => {
    expect(rowsFor("browse").map((r) => r.id)).not.toContain("app.reload");
    expect(rowsFor("browse", { dev: true }).map((r) => r.id)).toContain("app.reload");
  });

  it("has no viewer row for Delete, E, Ctrl+A or Ctrl+D", () => {
    const keys = rowsFor("viewer").flatMap((r) => r.keys);
    for (const k of ["Delete", "E", "Ctrl+A", "Ctrl+D"]) expect(keys).not.toContain(k);
  });
});

describe("parseKeymap", () => {
  const ok = { id: "browse.star", place: "browse", keys: ["F"] };

  it("accepts a good row", () => {
    expect(parseKeymap([ok])[0]?.keys).toEqual(["F"]);
  });

  it.each([
    ["not an array", { rows: [] }],
    ["a row that is not an object", ["browse.star"]],
    ["an unknown field", [{ ...ok, label: "Star" }]],
    ["an unknown place", [{ ...ok, place: "grid" }]],
    ["no keys", [{ ...ok, keys: [] }]],
    ["a flag that is not a boolean", [{ ...ok, hold: "yes" }]],
    ["a duplicate id", [ok, { ...ok, keys: ["S"] }]],
    ["a lower-case letter", [{ ...ok, keys: ["f"] }]],
    ["an unknown key name", [{ ...ok, keys: ["Esc"] }]],
    ["a modifier spelled wrongly", [{ ...ok, keys: ["ctrl+F"] }]],
    ["modifiers out of order", [{ ...ok, keys: ["Shift+Ctrl+F"] }]],
    ["Meta", [{ ...ok, keys: ["Meta+F"] }]],
    ["a key used twice in one place", [ok, { id: "browse.hide", place: "browse", keys: ["F"] }]],
    [
      "a key that clashes with an everywhere row",
      [ok, { id: "app.shortcuts", place: "everywhere", keys: ["F"] }],
    ],
    ["a menu row with two keys", [{ ...ok, place: "everywhere", keys: ["F1", "F2"], menu: true }]],
    ["a development row that is not on the menu", [{ ...ok, dev: true }]],
    ["a row that is both hold and repeat", [{ ...ok, hold: true, repeat: true }]],
  ])("rejects %s", (_why, table) => {
    expect(() => parseKeymap(table)).toThrow(KeymapError);
  });

  it("allows the same key in different places", () => {
    expect(() =>
      parseKeymap([ok, { id: "viewer.star", place: "viewer", keys: ["F"] }]),
    ).not.toThrow();
  });

  it("allows a bare + key", () => {
    expect(parseKeymap([{ ...ok, keys: ["+"] }])[0]?.keys).toEqual(["+"]);
  });
});

describe("handlerGaps", () => {
  it("names rows with no handler and handlers with no row", () => {
    const gaps = handlerGaps(
      { "browse.star": () => {}, "browse.sparkle": () => {} },
      [row("browse.star"), row("browse.hide")],
    );
    expect(gaps).toEqual({ missing: ["browse.hide"], unknown: ["browse.sparkle"] });
  });

  it("is empty when every row has a handler", () => {
    expect(handlerGaps({ "browse.star": () => {} }, [row("browse.star")])).toEqual({
      missing: [],
      unknown: [],
    });
  });
});

/** The rows of the "Every key" table in docs/08, as keymap rows. */
function docRows() {
  // Page tests run from ui/.
  const path = resolve(process.cwd(), "..", "docs", "08-desktop-and-release.md");
  const doc = readFileSync(path, "utf8").replace(/\r\n/g, "\n");
  const start = doc.indexOf("### Every key");
  const end = doc.indexOf("\n### ", start + 1);
  const ticks = (cell: string) => [...cell.matchAll(/`([^`]+)`/g)].map((m) => m[1] ?? "");
  const rows: Record<string, unknown>[] = [];
  for (const line of doc.slice(start, end).split("\n")) {
    if (!line.startsWith("| `")) continue;
    const cells = line.split(" | ").map((c) => c.replace(/^\| /, "").replace(/ \|$/, ""));
    const [idCell = "", place = "", keyCell = "", , flagCell = ""] = cells;
    const ids = ticks(idCell);
    const flags = flagCell.split(/,\s*/).filter(Boolean);
    const groups = ids.length === 1 ? [ticks(keyCell)] : keyCell.split(", ").map(ticks);
    expect(groups.length, `keys for ${idCell}`).toBe(ids.length);
    ids.forEach((id, i) =>
      rows.push({
        id,
        place,
        keys: groups[i],
        hold: flags.includes("hold"),
        repeat: flags.includes("repeat"),
        menu: flags.includes("menu"),
        dev: flags.includes("dev"),
        typing: flags.includes("typing"),
      }),
    );
  }
  return rows;
}
