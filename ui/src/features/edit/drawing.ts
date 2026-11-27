// How the page draws an edit plan (docs/06-editing-and-file-actions.md,
// "How the page draws a plan"). The plan comes from Rust; the page only
// multiplies its numbers by a fit factor. `fixtures/edit-plans.json`, which
// a Rust test writes, pins these strings to the model.

import type { EditPlan } from "../../api/bindings/EditPlan";

/** `contain`: the whole output fits the box. `cover`: it covers the box, which cuts the overflow. */
export type Fit = "contain" | "cover";

/** What `EditedImage` sets for a plan in a box. */
export interface Drawn {
  /** The outer element's size. */
  outerW: number;
  outerH: number;
  /** The `<img>`'s size. */
  width: number;
  height: number;
  /** The `<img>`'s CSS transform. */
  transform: string;
  /** The colour filter's `values`; null without a colour change. */
  filter: string | null;
}

/** Rounds to four decimal places, half away from zero as Rust does, with no `-0`. */
export function round4(v: number): number {
  const r = (Math.sign(v) * Math.round(Math.abs(v) * 10_000)) / 10_000;
  return r === 0 ? 0 : r;
}

/** The drawing rules for `plan` in a `boxW × boxH` box. */
export function drawPlan(plan: EditPlan, boxW: number, boxH: number, fit: Fit): Drawn {
  const ow = plan.out.w;
  const oh = plan.out.h;
  const f = fit === "contain" ? Math.min(boxW / ow, boxH / oh) : Math.max(boxW / ow, boxH / oh);
  const width = plan.pic.w * f;
  const height = plan.pic.h * f;
  const tx = (plan.frame.w / 2 - plan.crop.left) * f;
  const ty = (plan.frame.h / 2 - plan.crop.top) * f;
  let transform = `translate(${round4(tx)}px, ${round4(ty)}px) rotate(${round4(plan.angle)}deg)`;
  if (plan.flip) transform += " scaleX(-1)";
  transform += ` translate(${round4(-width / 2)}px, ${round4(-height / 2)}px)`;
  return {
    outerW: round4(ow * f),
    outerH: round4(oh * f),
    width: round4(width),
    height: round4(height),
    transform,
    filter: plan.colour === null ? null : plan.colour.svg.map((v) => String(v)).join(" "),
  };
}

/** The id of the SVG filter for an edit key. */
export const filterId = (key: string) => `owlbum-colour-${key}`;
