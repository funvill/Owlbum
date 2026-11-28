import { useId } from "react";
import { Icon } from "../icons";
import "./ProgressBar.css";

interface ProgressBarProps {
  /** Shown after the bar, such as "Reading 1,240 of 5,000". */
  label: string;
  /** Leave out, with max, when the total is not known. */
  value?: number;
  max?: number;
  /** Stopped on a problem: the label is drawn in the error colour with an alert icon. */
  failed?: boolean;
  /** The bar's accessible name, when it is not the label. */
  name?: string;
}

/** A 6 px bar with a label. docs/09-visual-design.md, "Progress bars". */
export function ProgressBar({ label, value, max, failed = false, name }: ProgressBarProps) {
  const labelId = useId();
  const known = value !== undefined && max !== undefined && max > 0;
  const classes = ["ow-progress", !known && "ow-progress--unknown", failed && "ow-progress--failed"]
    .filter(Boolean)
    .join(" ");
  return (
    <div className={classes}>
      <div
        role="progressbar"
        aria-labelledby={name === undefined ? labelId : undefined}
        aria-label={name}
        aria-valuemin={known ? 0 : undefined}
        aria-valuenow={known ? value : undefined}
        aria-valuemax={known ? max : undefined}
        className="ow-progress__track"
      >
        <div
          className="ow-progress__fill"
          style={known ? { width: `${(Math.min(value, max) / max) * 100}%` } : undefined}
        />
      </div>
      <span id={labelId} className="ow-progress__label">
        {failed && <Icon name="alert" size="sm" />}
        {label}
      </span>
    </div>
  );
}
