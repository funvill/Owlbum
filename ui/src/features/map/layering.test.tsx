// The map under every layer above it (docs/04-browsing.md, "Layers";
// docs/09-visual-design.md, "Layering"; #146). jsdom applies no style
// sheets, so the stacking order is computed here from the page's own CSS
// and the map library's: each element's chain of stacking contexts, from
// the classes it and its ancestors carry, compared where the chains part.

import { render, waitFor } from "@testing-library/react";
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { join } from "node:path";
import { setApi } from "../../api";
import { createFakeApi, type FakeApi } from "../../api/fake";
import { createQueryFake } from "../../api/fake/query";
import { Dialog, Menu, showToast, Toasts, useToasts } from "../../controls";
import { declarations, firstBlock, read, sourceFiles, srcDir, tokenFile } from "../../test/sources";
import { useSettingsStore } from "../app";
import { resetGrid, useGridStore } from "../grid";
import { resetViewer, useViewerStore, Viewer } from "../viewer";
import { MapView, resetMapPlace } from "./MapView";

/** What a class sets that makes a stacking context. */
interface Rule {
  position?: string;
  zIndex?: string;
  isolate?: boolean;
}

const tokens = declarations(firstBlock(read(tokenFile)));

/** The rules of single-class selectors in `css`, by class name. */
function rulesOf(css: string, into: Map<string, Rule>): void {
  const plain = css.replace(/\/\*[\s\S]*?\*\//g, "");
  for (const m of plain.matchAll(/([^{}@]+)\{([^{}]*)\}/g)) {
    const body = m[2]!;
    const decl = (name: string) => new RegExp(`(?:^|[;\\s])${name}\\s*:\\s*([^;]+)`).exec(body)?.[1]?.trim();
    const position = decl("position");
    const zIndex = decl("z-index");
    const isolate = decl("isolation") === "isolate";
    if (position === undefined && zIndex === undefined && !isolate) continue;
    for (const selector of m[1]!.split(",").map((s) => s.trim())) {
      const single = /^\.([\w-]+)$/.exec(selector);
      if (single === null) continue;
      const had = into.get(single[1]!) ?? {};
      into.set(single[1]!, {
        ...had,
        ...(position === undefined ? {} : { position }),
        ...(zIndex === undefined ? {} : { zIndex }),
        ...(isolate ? { isolate } : {}),
      });
    }
  }
}

const rules = new Map<string, Rule>();
const libraryCss = [
  join(srcDir, "..", "node_modules", "leaflet", "dist", "leaflet.css"),
  join(srcDir, "..", "node_modules", "leaflet.markercluster", "dist", "MarkerCluster.css"),
];
for (const file of libraryCss) rulesOf(read(file), rules);
for (const file of sourceFiles([".css"])) rulesOf(read(join(srcDir, file)), rules);

/** The z-index a rule gives, with tokens read from the token file. */
function zOf(rule: Rule): number | null {
  const z = rule.zIndex;
  if (z === undefined || z === "auto") return null;
  const token = /^var\((--[\w-]+)\)$/.exec(z);
  const value = token === null ? z : tokens.get(token[1]!);
  if (value === undefined) throw new Error(`Unknown z-index ${z}`);
  return Number(value);
}

/** An element's stacking contexts, outermost first, each with its z-index (0 for an isolated one without). */
function contexts(el: Element): { el: Element; z: number }[] {
  const chain: { el: Element; z: number }[] = [];
  for (let at: Element | null = el; at !== null; at = at.parentElement) {
    const rule: Rule = {};
    for (const c of at.classList) Object.assign(rule, rules.get(c));
    const z = zOf(rule);
    const positioned = rule.position !== undefined && rule.position !== "static";
    if ((positioned && z !== null) || rule.isolate === true) chain.unshift({ el: at, z: positioned && z !== null ? z : 0 });
  }
  return chain;
}

/** Whether `top` is drawn above `below`: compared where their chains of stacking contexts part. */
function covers(top: Element, below: Element): boolean {
  const a = contexts(top);
  const b = contexts(below);
  let i = 0;
  while (i < a.length && i < b.length && a[i]!.el === b[i]!.el) i++;
  const za = a[i]?.z ?? 0;
  const zb = b[i]?.z ?? 0;
  if (za !== zb) return za > zb;
  // Equal: the later one in the document is drawn on top.
  const x = a[i]?.el ?? top;
  const y = b[i]?.el ?? below;
  return (y.compareDocumentPosition(x) & Node.DOCUMENT_POSITION_FOLLOWING) !== 0;
}

beforeAll(() => {
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  );
});

afterAll(() => vi.unstubAllGlobals());

let fake: FakeApi;

beforeEach(() => {
  resetGrid();
  resetMapPlace();
  resetViewer();
  useToasts.setState({ toasts: [] });
  useSettingsStore.setState({ settings: null, error: null });
  fake = createFakeApi(
    createQueryFake([
      { id: 1, rel_path: "a.jpg", place: [49.25, -123.1] },
      { id: 2, rel_path: "b.jpg", place: [-33.9, 151.2] },
    ]).handlers,
  );
  setApi(fake);
});

afterEach(() => {
  resetViewer();
  resetGrid();
});

describe("the map's layering", () => {
  it("reads the stacking rules of the page and the map library", () => {
    expect(zOf(rules.get("ow-viewer")!)).toBe(300);
    expect(rules.get("ow-map__frame")?.isolate).toBe(true);
    // The library's controls sit at 1000, far above every layer token.
    expect(zOf(rules.get("leaflet-top")!)).toBe(1000);
  });

  it("is covered by the viewer, a dialog, a menu and a toast", async () => {
    render(
      <>
        <MapView album="/photos" />
        <Viewer detail={null} />
        <Dialog open title="A dialog" onClose={() => {}}>
          Text
        </Dialog>
        <Menu label="A menu" at={{ x: 10, y: 10 }} entries={[{ id: "a", label: "One", onSelect: () => {} }]} onClose={() => {}} />
        <Toasts />
      </>,
    );
    const grid = useGridStore.getState();
    await grid.start();
    grid.setQuery({ ...grid.query, view: "map" });
    await waitFor(() => expect(fake.calls.some((c) => c.command === "query_map")).toBe(true));
    showToast({ kind: "info", message: "A toast" });
    useViewerStore.setState({ open: true });

    const find = (selector: string) => {
      const el = document.querySelector(selector);
      expect(el, selector).not.toBeNull();
      return el!;
    };
    const map = [".ow-map__stage", ".leaflet-marker-pane", ".leaflet-top", ".ow-map__toggle, .ow-map__consent"].map(find);
    await waitFor(() => find(".ow-viewer"));
    const above = [".ow-viewer", ".ow-dialog-layer", ".ow-menu", ".ow-toasts"].map(find);
    for (const top of above) {
      for (const below of map) {
        expect(covers(top, below), `${top.className} over ${below.className}`).toBe(true);
      }
    }
    // The check can fail: the library's controls do cover the map's own stage.
    expect(covers(find(".leaflet-top"), find(".leaflet-marker-pane"))).toBe(true);
    expect(covers(find(".ow-map__stage"), find(".ow-viewer"))).toBe(false);
  });
});
