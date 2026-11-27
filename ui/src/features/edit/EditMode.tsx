// Edit mode's stage and toolbar, inside the preview pane
// (docs/06-editing-and-file-actions.md, "Edit mode", "Layout", "The stage
// and the crop box", "Toolbar", "Keys in edit mode").
//
// The stage draws the medium thumbnail through the draft's whole-frame
// plan, with the crop box placed over it in fractions of the frame. The
// toolbar leaves room after Done for the save buttons that saving adds.

import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type MouseEvent as ReactMouseEvent,
  type PointerEvent as ReactPointerEvent,
  type RefObject,
} from "react";
import { thumbUrl } from "../../api";
import type { Colour } from "../../api/bindings/Colour";
import type { CropRect } from "../../api/bindings/CropRect";
import type { EditPlan } from "../../api/bindings/EditPlan";
import { Button, IconButton, Segmented, Slider, Tooltip } from "../../controls";
import { matchKey, primaryKey } from "../../keys";
import { editText as t } from "../../text/edit";
import { previewText } from "../../text/preview";
import { beforeButtonHandlers, holdBefore, useBeforeStore } from "./before";
import { ASPECTS, HANDLES, aspectRatio, canSwap, resizeCrop, type Handle } from "./crop";
import { drawPlan } from "./drawing";
import { EditedImage } from "./EditedImage";
import {
  autoColour,
  chooseAspect,
  finishEdit,
  resetDraft,
  setColour,
  setCrop,
  setStraighten,
  swapAspect,
  transformDraft,
  useEditStore,
} from "./mode";
import "./EditMode.css";

/** Room kept round the frame so its handles can be grabbed at the edge. */
export const STAGE_MARGIN = 24;

/** The keys of edit mode, while it is open. */
function useEditKeys(): void {
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.ctrlKey || e.altKey || e.metaKey) return;
      const dialog = document.querySelector('[aria-modal="true"]') !== null;
      const match = matchKey(e, { place: "edit", focus: dialog ? "dialog" : undefined });
      if (match === null || match.row.place === "everywhere") return;
      e.preventDefault();
      if (!match.act) return;
      if (match.id === "edit.done") void finishEdit("done");
      else if (match.id === "edit.cancel") void finishEdit("cancel");
      else if (match.id === "edit.rotate_left") void transformDraft("rotate_left");
      else if (match.id === "edit.rotate_right") void transformDraft("rotate_right");
      else if (match.id === "edit.before") holdBefore();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);
}

/** The stage's size, followed as the pane changes. */
function useSize(ref: RefObject<HTMLElement | null>): { width: number; height: number } {
  const [size, setSize] = useState({ width: 0, height: 0 });
  useLayoutEffect(() => {
    const el = ref.current;
    if (el === null) return;
    const measure = () => {
      const next = { width: el.clientWidth, height: el.clientHeight };
      setSize((old) => (old.width === next.width && old.height === next.height ? old : next));
    };
    measure();
    if (typeof ResizeObserver === "undefined") {
      window.addEventListener("resize", measure);
      return () => window.removeEventListener("resize", measure);
    }
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, [ref]);
  return size;
}

export function EditMode() {
  const stageRef = useRef<HTMLDivElement>(null);
  useEditKeys();
  // Focus starts on the stage, so Enter means Done and Esc means Cancel.
  useEffect(() => stageRef.current?.focus(), []);
  const keepFocusOnStage = (e: ReactMouseEvent) => {
    // A mouse click on a toolbar button does not leave focus on it (a
    // keyboard press has detail 0 and keeps it).
    if (e.detail > 0 && e.target instanceof Element && e.target.closest("button") !== null) stageRef.current?.focus();
  };
  return (
    <div className="ow-edit" role="region" aria-label={t.region}>
      <EditStage stageRef={stageRef} />
      <div className="ow-edit__toolbar" role="toolbar" aria-label={t.toolbar} onClick={keepFocusOnStage}>
        <EditToolbar />
      </div>
    </div>
  );
}

function EditStage({ stageRef }: { stageRef: RefObject<HTMLDivElement | null> }) {
  const plan = useEditStore((s) => s.plan);
  const thumbKey = useEditStore((s) => s.thumbKey);
  const before = useBeforeStore((s) => s.held);
  const size = useSize(stageRef);
  const boxW = Math.max(0, size.width - 2 * STAGE_MARGIN);
  const boxH = Math.max(0, size.height - 2 * STAGE_MARGIN);
  const known = plan !== null && boxW > 0 && boxH > 0;
  const src = thumbUrl(thumbKey, "m");

  return (
    <div
      ref={stageRef}
      className="ow-edit__stage"
      tabIndex={-1}
      aria-label={t.stage}
      onContextMenu={(e) => e.preventDefault()}
    >
      {known && (before ? <Original plan={plan} src={src} boxW={boxW} boxH={boxH} /> : <Frame plan={plan} src={src} boxW={boxW} boxH={boxH} />)}
    </div>
  );
}

/** Before: the plain upright picture, with no plan at all, and no crop box. */
function Original({ plan, src, boxW, boxH }: { plan: EditPlan; src: string; boxW: number; boxH: number }) {
  const f = Math.min(boxW / plan.pic.w, boxH / plan.pic.h);
  return (
    <>
      <img className="ow-edit__original" src={src} alt="" draggable={false} style={{ width: plan.pic.w * f, height: plan.pic.h * f }} />
      <span className="ow-zoom__original">{previewText.original}</span>
    </>
  );
}

/** The whole frame, drawn through the plan, with the crop box over it. */
function Frame({ plan, src, boxW, boxH }: { plan: EditPlan; src: string; boxW: number; boxH: number }) {
  const d = drawPlan(plan, boxW, boxH, "contain");
  return (
    <div className="ow-edit__frame" style={{ width: d.outerW, height: d.outerH }}>
      <EditedImage plan={plan} src={src} boxW={boxW} boxH={boxH} fit="contain" />
      <CropBox frameW={d.outerW} frameH={d.outerH} />
    </div>
  );
}

const pct = (v: number) => `${Number((v * 100).toFixed(4))}%`;

function CropBox({ frameW, frameH }: { frameW: number; frameH: number }) {
  const crop = useEditStore((s) => s.draft.crop);
  const drag = useRef<{ id: number; handle: Handle; x: number; y: number; from: CropRect } | null>(null);

  // One listener for the box and its handles: a handle names itself in `data-handle`.
  const start = (e: ReactPointerEvent<HTMLElement>) => {
    if (e.button !== 0) return;
    const named = e.target instanceof HTMLElement ? e.target.dataset.handle : undefined;
    const handle = (HANDLES as readonly string[]).includes(named ?? "") ? (named as Handle) : "move";
    e.preventDefault();
    e.stopPropagation();
    drag.current = { id: e.pointerId, handle, x: e.clientX, y: e.clientY, from: useEditStore.getState().draft.crop };
    e.currentTarget.setPointerCapture?.(e.pointerId);
  };
  const move = (e: ReactPointerEvent<HTMLElement>) => {
    const d = drag.current;
    if (d === null || d.id !== e.pointerId || frameW <= 0 || frameH <= 0) return;
    const { plan, aspect, swapped } = useEditStore.getState();
    if (plan === null) return;
    const ratio = aspectRatio(aspect, swapped, plan.frame);
    setCrop(resizeCrop(d.from, d.handle, (e.clientX - d.x) / frameW, (e.clientY - d.y) / frameH, ratio, plan.frame));
  };
  const end = (e: ReactPointerEvent<HTMLElement>) => {
    if (drag.current?.id === e.pointerId) drag.current = null;
  };

  return (
    <div
      className="ow-crop"
      role="group"
      aria-label={t.crop}
      style={{ left: pct(crop.x), top: pct(crop.y), width: pct(crop.w), height: pct(crop.h) }}
      onPointerDown={start}
      onPointerMove={move}
      onPointerUp={end}
      onPointerCancel={end}
      onLostPointerCapture={end}
    >
      <span className="ow-crop__third ow-crop__third--v1" />
      <span className="ow-crop__third ow-crop__third--v2" />
      <span className="ow-crop__third ow-crop__third--h1" />
      <span className="ow-crop__third ow-crop__third--h2" />
      {HANDLES.map((h) => (
        <span
          key={h}
          className={`ow-crop__handle ow-crop__handle--${h}`}
          role="img"
          aria-label={t.handles[h]}
          data-handle={h}
        />
      ))}
    </div>
  );
}

/** A slider with a neutral 0 that a double-click returns to. */
function EditSlider({
  label,
  value,
  min,
  max,
  step,
  format,
  onChange,
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  format: (v: number) => string;
  onChange: (v: number) => void;
}) {
  return (
    <Tooltip text={t.resetSlider}>
      <span className="ow-edit__slider">
        <Slider label={label} value={value} min={min} max={max} step={step} neutral={0} format={format} onChange={onChange} />
      </span>
    </Tooltip>
  );
}

const COLOURS: (keyof Colour)[] = ["brightness", "contrast", "saturation", "warmth"];

function EditToolbar() {
  const draft = useEditStore((s) => s.draft);
  const aspect = useEditStore((s) => s.aspect);
  const swapped = useEditStore((s) => s.swapped);
  const plan = useEditStore((s) => s.plan);
  const swappable = plan !== null && canSwap(aspect, plan.frame);

  return (
    <>
      <div className="ow-edit__row">
        <Segmented
          label={t.aspect}
          value={aspect}
          options={ASPECTS.map((a) => ({ value: a, label: t.aspects[a], disabled: plan === null }))}
          onChange={(v) => chooseAspect(v as (typeof ASPECTS)[number])}
        />
        <IconButton
          icon="swap"
          label={t.swap}
          pressed={swappable ? swapped : undefined}
          disabledReason={swappable ? undefined : t.swapUnavailable}
          onClick={swapAspect}
        />
        <IconButton icon="rotate-ccw" label={t.rotateLeft} shortcut={primaryKey("edit.rotate_left")} onClick={() => void transformDraft("rotate_left")} />
        <IconButton icon="rotate-cw" label={t.rotateRight} shortcut={primaryKey("edit.rotate_right")} onClick={() => void transformDraft("rotate_right")} />
        <IconButton icon="flip-h" label={t.flip} onClick={() => void transformDraft("flip")} />
      </div>
      <div className="ow-edit__row">
        <EditSlider label={t.straighten} value={draft.straighten} min={-45} max={45} step={0.5} format={t.degrees} onChange={setStraighten} />
      </div>
      <div className="ow-edit__row ow-edit__row--colour">
        {COLOURS.map((c) => (
          <EditSlider key={c} label={t[c]} value={draft.colour[c]} min={-100} max={100} step={1} format={t.amount} onChange={(v) => setColour(c, v)} />
        ))}
      </div>
      <div className="ow-edit__row ow-edit__row--actions">
        <Tooltip text={t.autoTooltip}>
          <Button size="sm" icon="sliders" onClick={() => void autoColour()}>
            {t.auto}
          </Button>
        </Tooltip>
        <span className="ow-edit__gap" />
        <Tooltip text={previewText.beforeTooltip}>
          <Button size="sm" {...beforeButtonHandlers}>
            {t.before}
          </Button>
        </Tooltip>
        <Button size="sm" icon="reset" onClick={resetDraft}>
          {t.reset}
        </Button>
        <Tooltip text={primaryKey("edit.cancel") === "Escape" ? "Esc" : primaryKey("edit.cancel")}>
          <Button size="sm" onClick={() => void finishEdit("cancel")}>
            {t.cancel}
          </Button>
        </Tooltip>
        <Tooltip text={primaryKey("edit.done")}>
          <Button size="sm" kind="primary" icon="check" onClick={() => void finishEdit("done")}>
            {t.done}
          </Button>
        </Tooltip>
      </div>
    </>
  );
}
