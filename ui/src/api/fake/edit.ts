import type { EditCommands } from "../bindings/edit";
import type { EditPlan } from "../bindings/EditPlan";
import type { PhotoEdit } from "../bindings/PhotoEdit";
import type { AreaHandlers } from "./types";

/**
 * A plan for page tests: the whole `w × h` picture turned by `rotate`
 * (no straighten, crop or colour). `changes` replaces any field. The real
 * plan comes from Rust; the page never works one out.
 */
export function fakePlan(w = 4032, h = 3024, rotate = 0, changes: Partial<EditPlan> = {}): EditPlan {
  const turned = rotate === 90 || rotate === 270;
  const out = turned ? { w: h, h: w } : { w, h };
  return {
    key: `fake${rotate}`,
    pic: { w, h },
    flip: false,
    angle: rotate,
    frame: out,
    crop: { left: 0, top: 0, width: out.w, height: out.h },
    out,
    colour: null,
    ...changes,
  };
}

/** The edit that changes nothing. */
export const FAKE_IDENTITY: PhotoEdit = {
  rotate: 0,
  flip: false,
  straighten: 0,
  crop: { x: 0, y: 0, w: 1, h: 1 },
  colour: { brightness: 0, contrast: 0, saturation: 0, warmth: 0 },
};

/**
 * The fake backend's `edit_` commands: plans of a 4032 × 3024 photo; every
 * change succeeds and changes every photo sent.
 */
export const editHandlers: AreaHandlers<EditCommands> = {
  edit_plan: ({ edit }) => fakePlan(4032, 3024, edit.rotate),
  edit_transform: ({ edit, op }) => {
    if (op === "flip") return { ...edit, flip: !edit.flip, rotate: (360 - edit.rotate) % 360 };
    return { ...edit, rotate: (edit.rotate + (op === "rotate_right" ? 90 : 270)) % 360 };
  },
  edit_auto: () => ({ brightness: 12, contrast: 8, saturation: 0, warmth: -5 }),
  edit_set: ({ edit }) => ({ changed: true, plan: edit === null ? null : fakePlan(4032, 3024, edit.rotate), revision: 2 }),
  edit_turn: ({ photo_ids }) => ({ changed: photo_ids, skipped: 0, revision: 2 }),
  edit_revert: ({ photo_ids }) => ({ changed: photo_ids, revision: 2 }),
};
