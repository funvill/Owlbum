import type { ReactNode } from "react";
import { OwlMark } from "./OwlMark";
import "./EmptyState.css";

interface EmptyStateProps {
  heading: string;
  /** One sentence under the heading. */
  children?: ReactNode;
  /** At most one button. */
  action?: ReactNode;
  /** Shows the owl mark above the heading. */
  mark?: boolean;
}

/** A centred block for a place with nothing to show. docs/09-visual-design.md, "Empty states". */
export function EmptyState({ heading, children, action, mark = false }: EmptyStateProps) {
  return (
    <div className="ow-empty">
      {mark && <OwlMark />}
      <h2 className="ow-empty__heading">{heading}</h2>
      {children !== undefined && <p className="ow-empty__text">{children}</p>}
      {action}
    </div>
  );
}
