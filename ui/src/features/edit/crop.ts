// The crop box's pointer geometry (docs/06-editing-and-file-actions.md,
// "Aspect presets", "Minimum crop sizes", "The stage and the crop box").
//
// The page owns only this: a candidate crop in fractions of the frame.
// Whatever it produces goes through `parse_edit` and `edit_plan` in Rust
// before anything is drawn from it or kept.

import type { CropRect } from "../../api/bindings/CropRect";

/** The frame's size in pixels (the whole-frame plan's `frame`). */
export interface FrameSize {
  w: number;
  h: number;
}

/** The smallest crop the handles can make, each way, in fractions of the frame. */
export const MIN_CROP = 0.05;

/** The aspect presets, in the toolbar's order. */
export const ASPECTS = ["free", "original", "square", "4:5", "3:2", "16:9"] as const;
export type Aspect = (typeof ASPECTS)[number];

/** Fixed ratios (width ÷ height) of the presets that have one of their own. */
const FIXED: Partial<Record<Aspect, number>> = { square: 1, "4:5": 0.8, "3:2": 1.5, "16:9": 16 / 9 };

/** Whether the preset swaps with the quarter turns (the box turns with the photo). */
export const turnsWithPhoto = (aspect: Aspect) => aspect === "4:5" || aspect === "3:2" || aspect === "16:9";

/** Whether Swap is offered: 4:5, 3:2, 16:9, and Original on a photo that is not square. */
export function canSwap(aspect: Aspect, frame: FrameSize | null): boolean {
  if (turnsWithPhoto(aspect)) return true;
  return aspect === "original" && frame !== null && frame.w !== frame.h;
}

/** The preset's ratio (width ÷ height), the inverse when swapped; null for Free. */
export function aspectRatio(aspect: Aspect, swapped: boolean, frame: FrameSize): number | null {
  if (aspect === "free") return null;
  const r = aspect === "original" ? frame.w / frame.h : FIXED[aspect]!;
  return swapped && canSwap(aspect, frame) ? 1 / r : r;
}

/** A ratio in fractions of the frame: crop.w ÷ crop.h for that ratio in pixels. */
export const fractionRatio = (ratio: number, frame: FrameSize) => (ratio * frame.h) / frame.w;

const clamp = (v: number, lo: number, hi: number) => Math.min(Math.max(v, lo), Math.max(lo, hi));

/**
 * The largest rectangle of `ratio`, centred on the old crop's centre and
 * moved to stay inside the frame.
 */
export function fitCropToAspect(crop: CropRect, ratio: number, frame: FrameSize): CropRect {
  const rf = fractionRatio(ratio, frame);
  const h = Math.min(1, 1 / rf);
  const w = h * rf;
  const cx = crop.x + crop.w / 2;
  const cy = crop.y + crop.h / 2;
  return { x: clamp(cx - w / 2, 0, 1 - w), y: clamp(cy - h / 2, 0, 1 - h), w, h };
}

/** What is dragged: the box itself, a side, or a corner. */
export type Handle = "move" | "n" | "s" | "e" | "w" | "nw" | "ne" | "sw" | "se";

/** The eight handles, corners first, as the stage draws them. */
export const HANDLES = ["nw", "ne", "sw", "se", "n", "s", "w", "e"] as const satisfies readonly Handle[];

/**
 * The crop after dragging `handle` by `dx`, `dy` (fractions of the frame)
 * from `start`. `ratio` is the preset's ratio in pixels, null for Free.
 * The box stays inside the frame and never under 0.05 each way.
 */
export function resizeCrop(
  start: CropRect,
  handle: Handle,
  dx: number,
  dy: number,
  ratio: number | null,
  frame: FrameSize,
): CropRect {
  const left = start.x;
  const top = start.y;
  const right = start.x + start.w;
  const bottom = start.y + start.h;

  if (handle === "move") {
    return { x: clamp(left + dx, 0, 1 - start.w), y: clamp(top + dy, 0, 1 - start.h), w: start.w, h: start.h };
  }

  const west = handle.includes("w");
  const east = handle.includes("e");
  const north = handle.includes("n");
  const south = handle.includes("s");

  if (ratio === null) {
    // Each dragged edge on its own.
    const l = west ? clamp(left + dx, 0, right - MIN_CROP) : left;
    const r = east ? clamp(right + dx, left + MIN_CROP, 1) : right;
    const t = north ? clamp(top + dy, 0, bottom - MIN_CROP) : top;
    const b = south ? clamp(bottom + dy, top + MIN_CROP, 1) : bottom;
    return { x: l, y: t, w: r - l, h: b - t };
  }

  const rf = fractionRatio(ratio, frame);
  const minW = Math.max(MIN_CROP, MIN_CROP * rf);
  const minH = minW / rf;

  if ((west || east) && (north || south)) {
    // A corner: the opposite corner stays put.
    const fx = west ? right : left;
    const fy = north ? bottom : top;
    const dragW = west ? start.w - dx : start.w + dx;
    const dragH = north ? start.h - dy : start.h + dy;
    const roomW = west ? fx : 1 - fx;
    const roomH = north ? fy : 1 - fy;
    const w = clamp(Math.min(dragW, dragH * rf), minW, Math.min(roomW, roomH * rf));
    const h = w / rf;
    return { x: west ? fx - w : fx, y: north ? fy - h : fy, w, h };
  }

  if (west || east) {
    // A side: the opposite side stays, the height follows about the old centre.
    const cy = top + start.h / 2;
    const dragW = west ? start.w - dx : start.w + dx;
    const room = west ? right : 1 - left;
    const w = clamp(dragW, minW, Math.min(room, 2 * Math.min(cy, 1 - cy) * rf));
    const h = w / rf;
    return { x: west ? right - w : left, y: cy - h / 2, w, h };
  }

  const cx = left + start.w / 2;
  const dragH = north ? start.h - dy : start.h + dy;
  const room = north ? bottom : 1 - top;
  const h = clamp(dragH, minH, Math.min(room, (2 * Math.min(cx, 1 - cx)) / rf));
  const w = h * rf;
  return { x: cx - w / 2, y: north ? bottom - h : top, w, h };
}
