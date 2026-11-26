import { useRef, type KeyboardEvent } from "react";
import { Icon, type IconName } from "../icons";
import "./Segmented.css";

export interface SegmentOption {
  value: string;
  label: string;
  icon?: IconName;
  disabled?: boolean;
}

interface SegmentedProps {
  label: string;
  value: string;
  options: SegmentOption[];
  onChange: (value: string) => void;
  /** A tab list where it switches the grid, timeline and map views; a radio group otherwise. */
  as?: "radiogroup" | "tablist" | undefined;
}

/**
 * A row of choices where exactly one is on. It is one Tab stop; the arrow
 * keys, Home and End move and choose. docs/09-visual-design.md,
 * "Segmented control".
 */
export function Segmented({ label, value, options, onChange, as = "radiogroup" }: SegmentedProps) {
  const buttons = useRef<Map<string, HTMLButtonElement>>(new Map());
  const enabled = options.filter((o) => o.disabled !== true);
  const itemRole = as === "tablist" ? "tab" : "radio";

  const choose = (next: SegmentOption | undefined) => {
    if (next === undefined) return;
    onChange(next.value);
    buttons.current.get(next.value)?.focus();
  };

  const onKeyDown = (e: KeyboardEvent) => {
    const at = enabled.findIndex((o) => o.value === value);
    const step = (by: number) => enabled[(at + by + enabled.length) % enabled.length];
    const moves: Record<string, () => SegmentOption | undefined> = {
      ArrowRight: () => step(1),
      ArrowDown: () => step(1),
      ArrowLeft: () => step(-1),
      ArrowUp: () => step(-1),
      Home: () => enabled[0],
      End: () => enabled[enabled.length - 1],
    };
    const move = moves[e.key];
    if (move === undefined) return;
    e.preventDefault();
    choose(move());
  };

  return (
    <div role={as} aria-label={label} className="ow-segmented" onKeyDown={onKeyDown}>
      {options.map((option) => {
        const on = option.value === value;
        return (
          <button
            key={option.value}
            ref={(el) => {
              if (el === null) buttons.current.delete(option.value);
              else buttons.current.set(option.value, el);
            }}
            type="button"
            role={itemRole}
            aria-checked={itemRole === "radio" ? on : undefined}
            aria-selected={itemRole === "tab" ? on : undefined}
            tabIndex={on ? 0 : -1}
            disabled={option.disabled}
            className="ow-segmented__segment"
            onClick={() => choose(option)}
          >
            {option.icon !== undefined && <Icon name={option.icon} />}
            {option.label}
          </button>
        );
      })}
    </div>
  );
}
