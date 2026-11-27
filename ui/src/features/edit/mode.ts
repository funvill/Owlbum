// Edit mode (docs/06-editing-and-file-actions.md, "Edit mode").
//
// One photo is edited at a time, inside the preview pane. The draft lives
// here, in the page, until Done stores it with `edit_set`. Rust owns every
// number that turns the draft into pixels: the stage draws the whole-frame
// plan `edit_plan` answers, the quarter turns and the flip are
// `edit_transform`, and Auto is `edit_auto`. The page owns only the crop
// box's geometry (./crop.ts), the slider positions and the aspect choice.

import { create } from "zustand";
import { api, toAppError } from "../../api";
import type { Colour } from "../../api/bindings/Colour";
import type { CropRect } from "../../api/bindings/CropRect";
import type { EditPlan } from "../../api/bindings/EditPlan";
import type { PhotoEdit } from "../../api/bindings/PhotoEdit";
import type { Row } from "../../api/bindings/Row";
import type { TurnOp } from "../../api/bindings/TurnOp";
import { showToast } from "../../controls/toastQueue";
import { setEditing } from "../../keys";
import { editText as t } from "../../text/edit";
import { BLOCK, gridBlocks, useGridStore } from "../grid";
import { canChange } from "../organise";
import { aspectRatio, canSwap, fitCropToAspect, turnsWithPhoto, type Aspect } from "./crop";
import { releaseBefore } from "./before";

/** The edit that changes nothing. */
export const IDENTITY: PhotoEdit = {
  rotate: 0,
  flip: false,
  straighten: 0,
  crop: { x: 0, y: 0, w: 1, h: 1 },
  colour: { brightness: 0, contrast: 0, saturation: 0, warmth: 0 },
};

export interface EditModeState {
  /** The photo being edited; null while edit mode is closed. */
  photoId: number | null;
  /** The thumbnail key of that photo, for the stage's picture. */
  thumbKey: string;
  draft: PhotoEdit;
  aspect: Aspect;
  swapped: boolean;
  /** The draft's whole-frame plan (the crop ignored); null until the first answer. */
  plan: EditPlan | null;
}

const closed: EditModeState = {
  photoId: null,
  thumbKey: "",
  draft: IDENTITY,
  aspect: "free",
  swapped: false,
  plan: null,
};

export const useEditStore = create<EditModeState>(() => closed);

/** The row of photo `id` among the rows the grid holds. */
export function heldRow(id: number): Row | undefined {
  const blocks = gridBlocks();
  for (const b of blocks.held()) {
    for (let i = b * BLOCK; i < (b + 1) * BLOCK; i++) {
      const row = blocks.row(i);
      if (row?.id === id) return row;
    }
  }
  return undefined;
}

/** Why `row` cannot be edited (docs/06, "Formats"); null when it can. */
export function notEditable(row: Pick<Row, "kind" | "duration_ms">): string | null {
  if (row.kind === "video") return t.video;
  return row.duration_ms !== null ? t.animated : null;
}

/** Whether edit mode is open (on any photo). */
export const isEditing = () => useEditStore.getState().photoId !== null;

let unfollow: (() => void) | null = null;

function close(): void {
  unfollow?.();
  unfollow = null;
  setEditing(false);
  releaseBefore();
  useEditStore.setState(closed, true);
}

/**
 * Opens edit mode on photo `id`, which must be the current photo. Another
 * photo's edit mode ends as Done first. Does nothing in a view-only album,
 * for a photo that cannot be edited, or when the photo stops being current
 * before its stored edit arrives.
 */
export async function openEdit(id: number): Promise<void> {
  if (!canChange()) return;
  const open = useEditStore.getState().photoId;
  if (open === id) return;
  if (open !== null && !(await finishEdit("done"))) return;
  const row = heldRow(id);
  if (row === undefined || notEditable(row) !== null) return;
  if (useGridStore.getState().current?.id !== id) return;
  let stored: PhotoEdit | null;
  try {
    stored = (await api().invoke("photo_detail", { id })).edit;
  } catch (e) {
    showToast({ kind: "error", message: toAppError(e).message });
    return;
  }
  if (useGridStore.getState().current?.id !== id || isEditing()) return;
  setEditing(true);
  useEditStore.setState({ ...closed, photoId: id, thumbKey: row.thumb_key, draft: stored ?? IDENTITY }, true);
  // Selecting another photo, or the photo leaving the result, ends edit mode as Done.
  unfollow = useGridStore.subscribe((g) => {
    if (g.current?.id !== id) void finishEdit("done", { leaving: true });
  });
  void askPlan();
}

let finishing: Promise<boolean> | null = null;

/**
 * Ends edit mode. Cancel keeps the stored edit as it was. Done stores the
 * draft with `edit_set`; on failure a toast says why and edit mode stays
 * open, unless it is ending because the photo is no longer shown
 * (`leaving`), when it closes anyway. Answers whether it closed.
 */
export function finishEdit(how: "done" | "cancel", { leaving = false } = {}): Promise<boolean> {
  const { photoId, draft } = useEditStore.getState();
  if (photoId === null) return Promise.resolve(true);
  if (how === "cancel") {
    close();
    return Promise.resolve(true);
  }
  if (finishing !== null) return finishing;
  const job = (async () => {
    try {
      await api().invoke("edit_set", { photo_id: photoId, edit: draft });
      if (useEditStore.getState().photoId === photoId) close();
      return true;
    } catch (e) {
      showToast({ kind: "error", message: t.keepFailed(toAppError(e).message) });
      if (leaving && useEditStore.getState().photoId === photoId) close();
      return leaving;
    } finally {
      finishing = null;
    }
  })();
  finishing = job;
  return job;
}

// Plans: at most one call in flight; a change meanwhile asks again for the newest draft.
let asking = false;
let again = false;

async function askPlan(): Promise<void> {
  if (asking) {
    again = true;
    return;
  }
  asking = true;
  try {
    do {
      again = false;
      const { photoId, draft } = useEditStore.getState();
      if (photoId === null) return;
      try {
        const plan = await api().invoke("edit_plan", { photo_id: photoId, edit: draft, whole: true });
        if (useEditStore.getState().photoId === photoId) useEditStore.setState({ plan });
      } catch {
        // The stage keeps the last plan; the next change asks again.
      }
    } while (again);
  } finally {
    asking = false;
  }
}

/** For tests: nothing open, nothing in flight. */
export function resetEditMode(): void {
  close();
  asking = false;
  again = false;
  finishing = null;
  chain = Promise.resolve();
}

/** Changes the draft; a change to the turn, flip, straighten or colour asks for a new plan. */
function change(next: Partial<PhotoEdit>, extra: Partial<EditModeState> = {}): void {
  const s = useEditStore.getState();
  if (s.photoId === null) return;
  useEditStore.setState({ draft: { ...s.draft, ...next }, ...extra });
  if (Object.keys(next).some((k) => k !== "crop")) void askPlan();
}

/** Button presses go one after another, so each turns the draft the last one made. */
let chain: Promise<unknown> = Promise.resolve();

/** Rotate left, rotate right or flip the draft (`edit_transform`). A 4:5, 3:2 or 16:9 box turns with the photo. */
export function transformDraft(op: TurnOp): Promise<void> {
  const job = async () => {
    const { photoId, draft, aspect, swapped } = useEditStore.getState();
    if (photoId === null) return;
    try {
      const next = await api().invoke("edit_transform", { edit: draft, op });
      if (useEditStore.getState().photoId !== photoId) return;
      const turned = op !== "flip" && turnsWithPhoto(aspect);
      change(next, turned ? { swapped: !swapped } : {});
    } catch (e) {
      showToast({ kind: "error", message: toAppError(e).message });
    }
  };
  const next = chain.then(job, job);
  chain = next.catch(() => {});
  return next;
}

export function setStraighten(straighten: number): void {
  change({ straighten });
}

export function setColour(field: keyof Colour, value: number): void {
  change({ colour: { ...useEditStore.getState().draft.colour, [field]: value } });
}

export function setCrop(crop: CropRect): void {
  change({ crop });
}

/** Chooses an aspect preset and refits the crop; the active one again changes nothing. */
export function chooseAspect(aspect: Aspect): void {
  const s = useEditStore.getState();
  if (s.photoId === null || s.plan === null || s.aspect === aspect) return;
  refit(aspect, false);
}

/** Swaps portrait and landscape, where the preset allows it, and refits. */
export function swapAspect(): void {
  const s = useEditStore.getState();
  if (s.photoId === null || s.plan === null || !canSwap(s.aspect, s.plan.frame)) return;
  refit(s.aspect, !s.swapped);
}

function refit(aspect: Aspect, swapped: boolean): void {
  const { plan, draft } = useEditStore.getState();
  if (plan === null) return;
  const ratio = aspectRatio(aspect, swapped, plan.frame);
  const crop = ratio === null ? draft.crop : fitCropToAspect(draft.crop, ratio, plan.frame);
  change({ crop }, { aspect, swapped });
}

/** The draft back to identity, Free, not swapped. */
export function resetDraft(): void {
  change({ ...IDENTITY }, { aspect: "free", swapped: false });
}

/** Auto: the four colour sliders from `edit_auto`; the rest of the draft is kept. */
export async function autoColour(): Promise<void> {
  const id = useEditStore.getState().photoId;
  if (id === null) return;
  try {
    const colour = await api().invoke("edit_auto", { photo_id: id });
    if (useEditStore.getState().photoId === id) change({ colour });
  } catch {
    showToast({ kind: "error", message: t.autoFailed });
  }
}
