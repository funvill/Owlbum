import type { ReactNode } from "react";
import { Icon } from "../icons";
import { controlsText } from "../text/controls";
import "./Chip.css";

interface ChipProps {
  label: string;
  /** A count chip ends with a number: "lake ×2". */
  count?: number;
  /** An active filter: accent fill, with an x that removes it. */
  active?: boolean;
  /** A plain chip that can be clicked, such as one that filters by its word. */
  onClick?: () => void;
  onRemove?: () => void;
}

/** A small rounded label. docs/09-visual-design.md, "Chips". */
export function Chip({ label, count, active = false, onClick, onRemove }: ChipProps) {
  const text = count === undefined ? label : controlsText.count(label, count);
  const classes = `ow-chip ${active ? "ow-chip--active" : "ow-chip--plain"}`;
  if (onClick !== undefined && !active) {
    return (
      <button type="button" className={`${classes} ow-chip--button`} onClick={onClick}>
        {text}
      </button>
    );
  }
  return (
    <span className={classes}>
      <span className="ow-chip__label">{text}</span>
      {onRemove !== undefined && (
        <button type="button" className="ow-chip__remove" aria-label={controlsText.removeChip(label)} onClick={onRemove}>
          <Icon name="x" size="sm" />
        </button>
      )}
    </span>
  );
}

export type PillTone = "badge" | "accent" | "edited" | "unsaved";

interface PillProps {
  tone?: PillTone;
  children: ReactNode;
}

/**
 * A pill drawn on or near a photo: the score, the stack count, a video's
 * duration, the edit badge. docs/09-visual-design.md, "Badges on tiles".
 */
export function Pill({ tone = "badge", children }: PillProps) {
  return <span className={`ow-pill ow-pill--${tone} ow-num`}>{children}</span>;
}
