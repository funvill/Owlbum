import { describe, expect, it } from "vitest";
import { render } from "@testing-library/react";
import { readDoc } from "../test/sources";
import { Icon, iconNames } from "./Icon";

/** The icon names in the table under "Icons" in docs/09-visual-design.md. */
function documentedNames(): string[] {
  const doc = readDoc("09-visual-design.md");
  const section = doc.slice(doc.indexOf("| Group | Icons |"), doc.indexOf("## Window sizes"));
  const rows = section.split("\n").filter((line) => line.startsWith("| ") && !line.startsWith("| Group"));
  const names: string[] = [];
  for (const row of rows) {
    const icons = row.split(" | ")[1] ?? "";
    // A name is a backticked word; words in brackets after it are notes.
    for (const match of icons.replace(/\([^)]*\)/g, "").matchAll(/`([a-z0-9-]+)`/g)) {
      names.push(match[1]!);
    }
  }
  return names;
}

function svgOf(container: HTMLElement): SVGSVGElement {
  const svg = container.querySelector("svg");
  if (svg === null) throw new Error("no svg");
  return svg;
}

describe("Icon", () => {
  it("has exactly the icons the design document lists", () => {
    const documented = documentedNames();
    expect(documented.length).toBe(65);
    expect([...iconNames].sort()).toEqual([...documented].sort());
  });

  it("draws one stroked path on a 24 unit grid, hidden from assistive technology", () => {
    const { container } = render(<Icon name="star" />);
    const svg = svgOf(container);
    expect(svg).toHaveAttribute("viewBox", "0 0 24 24");
    expect(svg).toHaveAttribute("aria-hidden", "true");
    expect(svg).toHaveAttribute("focusable", "false");
    expect(svg).toHaveAttribute("fill", "none");
    expect(svg).toHaveAttribute("stroke", "currentColor");
    expect(svg).toHaveAttribute("stroke-width", "1.7");
    expect(svg).toHaveAttribute("stroke-linecap", "round");
    expect(svg).toHaveAttribute("stroke-linejoin", "round");
    expect(svg.querySelectorAll("path")).toHaveLength(1);
  });

  it("is 16 px by default and takes the small and large sizes", () => {
    expect(svgOf(render(<Icon name="x" />).container)).toHaveAttribute("width", "16");
    expect(svgOf(render(<Icon name="x" size="sm" />).container)).toHaveAttribute("height", "14");
    expect(svgOf(render(<Icon name="x" size="lg" />).container)).toHaveAttribute("width", "20");
  });

  it.each(["star-filled", "play"] as const)("fills %s for its on state", (name) => {
    const svg = svgOf(render(<Icon name={name} />).container);
    expect(svg).toHaveAttribute("fill", "currentColor");
  });

  it("draws the filled star with the same outline as the star", () => {
    const outline = svgOf(render(<Icon name="star" />).container).querySelector("path");
    const filled = svgOf(render(<Icon name="star-filled" />).container).querySelector("path");
    expect(filled?.getAttribute("d")).toBe(outline?.getAttribute("d"));
  });

  it("draws rotate-ccw as its own drawing, not a mirrored rotate-cw", () => {
    const cw = svgOf(render(<Icon name="rotate-cw" />).container);
    const ccw = svgOf(render(<Icon name="rotate-ccw" />).container);
    expect(ccw.getAttribute("transform")).toBeNull();
    expect(ccw.querySelector("path")?.getAttribute("d")).not.toBe(cw.querySelector("path")?.getAttribute("d"));
  });

  it("refuses a name that is not in the set, at build time and at run time", () => {
    // @ts-expect-error -- "nope" is not an icon name; the type check fails without this line.
    const unknown = <Icon name="nope" />;
    expect(() => render(unknown)).toThrow(/nope/);
  });
});
