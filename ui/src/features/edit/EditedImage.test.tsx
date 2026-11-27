import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { render } from "@testing-library/react";
import type { EditPlan } from "../../api/bindings/EditPlan";
import { EditedImage } from "./EditedImage";
import { drawPlan, filterId, round4, type Fit } from "./drawing";
import { ColourFilters } from "./filters";

/** The shared plan fixtures a Rust test writes (crates/owlbum-core/src/edit/fixtures.rs). */
interface Fixture {
  name: string;
  box: { w: number; h: number };
  fit: Fit;
  plan: EditPlan;
  expect: { outer_w: number; outer_h: number; width: number; height: number; transform: string; filter: string | null };
}

const fixtures = JSON.parse(
  readFileSync(resolve(process.cwd(), "..", "fixtures", "edit-plans.json"), "utf8"),
) as Fixture[];

describe("EditedImage", () => {
  it("reads the shared fixtures, for contain and cover", () => {
    expect(fixtures.length).toBeGreaterThan(20);
    expect(new Set(fixtures.map((f) => f.fit))).toEqual(new Set(["contain", "cover"]));
  });

  it.each(fixtures.map((f) => [`${f.name}, ${f.fit} in ${f.box.w} × ${f.box.h}`, f] as const))(
    "draws %s exactly as the model says",
    (_, f) => {
      const d = drawPlan(f.plan, f.box.w, f.box.h, f.fit);
      expect(d).toEqual({
        outerW: f.expect.outer_w,
        outerH: f.expect.outer_h,
        width: f.expect.width,
        height: f.expect.height,
        transform: f.expect.transform,
        filter: f.expect.filter,
      });
      const { container, unmount } = render(
        <>
          <ColourFilters />
          <EditedImage plan={f.plan} src="owlbum://thumb/x/s" boxW={f.box.w} boxH={f.box.h} fit={f.fit} />
        </>,
      );
      // The strings above are exact; jsdom keeps three decimals of a
      // style it holds, so the element is compared with the same strings
      // passed through jsdom.
      const probe = document.createElement("div").style;
      const css = (prop: "transform" | "width" | "height", value: string) => {
        probe.setProperty(prop, value);
        return probe.getPropertyValue(prop);
      };
      const img = container.querySelector("img")!;
      expect(img.style.transform).toBe(css("transform", f.expect.transform));
      expect(img.style.width).toBe(css("width", `${f.expect.width}px`));
      expect(img.style.height).toBe(css("height", `${f.expect.height}px`));
      expect(img.style.transformOrigin).toBe("0px 0px");
      const outer = container.querySelector<HTMLElement>(".ow-edited")!;
      expect(outer.style.width).toBe(css("width", `${f.expect.outer_w}px`));
      expect(outer.style.overflow).toBe("hidden");
      const filter = container.querySelector(`filter#${filterId(f.plan.key)}`);
      if (f.expect.filter === null) {
        expect(img.style.filter).toBe("");
        expect(filter).toBeNull();
      } else {
        expect(img.style.filter.replace(/"/g, "")).toBe(`url(#${filterId(f.plan.key)})`);
        expect(filter).not.toBeNull();
        expect(filter!.getAttribute("color-interpolation-filters")).toBe("sRGB");
        expect(filter!.querySelector("feColorMatrix")!.getAttribute("values")).toBe(f.expect.filter);
        expect(filter!.querySelector("feColorMatrix")!.getAttribute("type")).toBe("matrix");
      }
      unmount();
    },
  );

  it("keeps one filter per key while any image uses it", () => {
    const f = fixtures.find((x) => x.expect.filter !== null)!;
    const { container, rerender } = render(
      <>
        <ColourFilters />
        <EditedImage plan={f.plan} src="a" boxW={10} boxH={10} fit="cover" />
        <EditedImage plan={f.plan} src="b" boxW={20} boxH={20} fit="cover" />
      </>,
    );
    expect(container.querySelectorAll("filter")).toHaveLength(1);
    rerender(
      <>
        <ColourFilters />
        <EditedImage plan={f.plan} src="a" boxW={10} boxH={10} fit="cover" />
      </>,
    );
    expect(container.querySelectorAll("filter")).toHaveLength(1);
    rerender(<ColourFilters />);
    expect(container.querySelectorAll("filter")).toHaveLength(0);
  });

  it("rounds half away from zero with no negative zero", () => {
    expect(round4(-0.00005)).toBe(-0.0001);
    expect(round4(0.00005)).toBe(0.0001);
    expect(Object.is(round4(-0.00001), 0)).toBe(true);
  });
});
