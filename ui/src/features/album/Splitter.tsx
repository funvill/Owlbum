import { useEffect, useRef, useState, type KeyboardEvent, type PointerEvent } from "react";

/** Left and Right move a splitter this far. */
export const SPLITTER_STEP = 16;
/** A width changed by keys is saved this long after the last key. */
export const SPLITTER_SAVE_MS = 500;
/**
 * A drag that goes this far past the minimum hides the panel when it is
 * let go (owner's feedback, issue 207), for splitters that can hide theirs.
 */
export const SPLITTER_COLLAPSE = 48;

export interface SplitterProps {
  /** The accessible name: "Resize thumbnail column". */
  label: string;
  /** The width the splitter sets, as shown. */
  value: number;
  min: number;
  max: number;
  /** +1 when moving right widens the column, −1 when it narrows it. */
  dir: 1 | -1;
  /** The width while it moves. */
  onMove(width: number): void;
  /** The width to keep: after a drag, after keys, or the default after a double-click. */
  onSave(width: number): void;
  /** The default width, put back by a double-click. */
  reset: number;
  /**
   * Hides the panel: a drag let go more than `SPLITTER_COLLAPSE` past the
   * minimum calls this instead of `onSave`, so the stored width is kept.
   */
  onCollapse?(): void;
  /**
   * A row splitter over the bottom strip: it sets a height, moves with the
   * pointer's height, and Up and Down move it (`dir` +1 when moving down
   * makes it bigger).
   */
  across?: boolean;
}

const clamp = (x: number, lo: number, hi: number) => Math.round(Math.min(hi, Math.max(lo, x)));

/**
 * A 6 px column splitter (docs/04-browsing.md, "Splitters"): a focusable
 * `separator` that a drag, Left and Right (16 px), Home and End move, and a
 * double-click puts back to its default. Across, it is a row splitter that
 * Up and Down move. A splitter given `onCollapse` hides its panel when a
 * drag is let go well past the minimum.
 */
export function Splitter({
  label,
  value,
  min,
  max,
  dir,
  onMove,
  onSave,
  reset,
  onCollapse,
  across = false,
}: SplitterProps) {
  const drag = useRef<{ x: number; start: number; width: number; collapse: boolean } | null>(null);
  const [dragging, setDragging] = useState(false);
  const [collapsing, setCollapsing] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const pendingKeys = useRef<number | null>(null);
  const save = useRef(onSave);
  useEffect(() => {
    save.current = onSave;
  });
  // A key move still waiting when the splitter goes is saved, not lost.
  useEffect(
    () => () => {
      clearTimeout(timer.current);
      if (pendingKeys.current !== null) save.current(pendingKeys.current);
    },
    [],
  );

  const end = () => {
    const d = drag.current;
    if (d === null) return;
    drag.current = null;
    setDragging(false);
    setCollapsing(false);
    if (d.collapse && onCollapse !== undefined) onCollapse();
    else onSave(d.width);
  };

  const onPointerDown = (e: PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    e.preventDefault();
    e.currentTarget.setPointerCapture?.(e.pointerId);
    drag.current = { x: across ? e.clientY : e.clientX, start: value, width: value, collapse: false };
    setDragging(true);
  };

  const onPointerMove = (e: PointerEvent<HTMLDivElement>) => {
    const d = drag.current;
    if (d === null) return;
    const raw = d.start + dir * ((across ? e.clientY : e.clientX) - d.x);
    d.width = clamp(raw, min, max);
    d.collapse = onCollapse !== undefined && raw < min - SPLITTER_COLLAPSE;
    setCollapsing(d.collapse);
    onMove(d.width);
  };

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    let width: number;
    if (e.key === (across ? "ArrowUp" : "ArrowLeft")) width = value - dir * SPLITTER_STEP;
    else if (e.key === (across ? "ArrowDown" : "ArrowRight")) width = value + dir * SPLITTER_STEP;
    else if (e.key === "Home") width = min;
    else if (e.key === "End" && Number.isFinite(max)) width = max;
    else return;
    e.preventDefault();
    e.stopPropagation();
    width = clamp(width, min, max);
    onMove(width);
    pendingKeys.current = width;
    clearTimeout(timer.current);
    timer.current = setTimeout(() => {
      pendingKeys.current = null;
      save.current(width);
    }, SPLITTER_SAVE_MS);
  };

  return (
    <div
      role="separator"
      aria-orientation={across ? "horizontal" : "vertical"}
      aria-label={label}
      aria-valuenow={value}
      aria-valuemin={min}
      aria-valuemax={Number.isFinite(max) ? max : undefined}
      tabIndex={0}
      className={`ow-splitter${across ? " ow-splitter--across" : ""}${dragging ? " ow-splitter--dragging" : ""}${collapsing ? " ow-splitter--collapsing" : ""}`}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={end}
      onLostPointerCapture={end}
      onKeyDown={onKeyDown}
      onDoubleClick={() => {
        clearTimeout(timer.current);
        pendingKeys.current = null;
        onMove(reset);
        onSave(reset);
      }}
    />
  );
}
