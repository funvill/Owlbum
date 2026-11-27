import { useEffect, useRef, useState, type KeyboardEvent, type PointerEvent } from "react";

/** Left and Right move a splitter this far. */
export const SPLITTER_STEP = 16;
/** A width changed by keys is saved this long after the last key. */
export const SPLITTER_SAVE_MS = 500;

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
}

const clamp = (x: number, lo: number, hi: number) => Math.round(Math.min(hi, Math.max(lo, x)));

/**
 * A 6 px column splitter (docs/04-browsing.md, "Splitters"): a focusable
 * `separator` that a drag, Left and Right (16 px), Home and End move, and a
 * double-click puts back to its default.
 */
export function Splitter({ label, value, min, max, dir, onMove, onSave, reset }: SplitterProps) {
  const drag = useRef<{ x: number; start: number; width: number } | null>(null);
  const [dragging, setDragging] = useState(false);
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
    onSave(d.width);
  };

  const onPointerDown = (e: PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    e.preventDefault();
    e.currentTarget.setPointerCapture?.(e.pointerId);
    drag.current = { x: e.clientX, start: value, width: value };
    setDragging(true);
  };

  const onPointerMove = (e: PointerEvent<HTMLDivElement>) => {
    const d = drag.current;
    if (d === null) return;
    d.width = clamp(d.start + dir * (e.clientX - d.x), min, max);
    onMove(d.width);
  };

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    let width: number;
    if (e.key === "ArrowLeft") width = value - dir * SPLITTER_STEP;
    else if (e.key === "ArrowRight") width = value + dir * SPLITTER_STEP;
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
      aria-orientation="vertical"
      aria-label={label}
      aria-valuenow={value}
      aria-valuemin={min}
      aria-valuemax={Number.isFinite(max) ? max : undefined}
      tabIndex={0}
      className={`ow-splitter${dragging ? " ow-splitter--dragging" : ""}`}
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
