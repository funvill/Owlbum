import type { ButtonHTMLAttributes, MouseEvent, ReactNode } from "react";
import { Icon, type IconName } from "../icons";
import { controlsText } from "../text/controls";
import { Tooltip } from "./Tooltip";
import "./Button.css";

type NativeProps = Omit<ButtonHTMLAttributes<HTMLButtonElement>, "disabled" | "type">;

interface ButtonProps extends NativeProps {
  /** Primary is the one main action of a view; in a dialog it is the last button. */
  kind?: "primary" | "default";
  size?: "md" | "sm";
  /** Deletes or overwrites: a default button with a red label, never primary. */
  danger?: boolean;
  icon?: IconName;
  /** Busy: keeps its width, shows a spinner in place of its icon, ignores clicks. */
  busy?: boolean;
  /**
   * Why the button cannot be used now. A disabled button stays focusable so
   * that its tooltip can say why. docs/09-visual-design.md, "Contrast".
   */
  disabledReason?: string;
  type?: "button" | "submit";
  children: ReactNode;
}

/** A small spinning ring, the size of a small icon. */
export function Spinner() {
  return (
    <svg className="ow-icon ow-spinner" width={14} height={14} viewBox="0 0 24 24" aria-hidden="true" focusable="false">
      <circle cx="12" cy="12" r="9" fill="none" stroke="currentColor" strokeWidth={2.5} strokeDasharray="42 100" strokeLinecap="round" />
    </svg>
  );
}

/** Blocks the click of a disabled or busy button. */
function guard(blocked: boolean, onClick: NativeProps["onClick"]) {
  return (e: MouseEvent<HTMLButtonElement>) => {
    if (blocked) {
      e.preventDefault();
      return;
    }
    onClick?.(e);
  };
}

/** A text button. docs/09-visual-design.md, "Buttons". */
export function Button({
  kind = "default",
  size = "md",
  danger = false,
  icon,
  busy = false,
  disabledReason,
  type = "button",
  className,
  onClick,
  children,
  ...rest
}: ButtonProps) {
  const disabled = disabledReason !== undefined;
  const shownKind = danger ? "default" : kind;
  const classes = [
    "ow-button",
    `ow-button--${shownKind}`,
    size === "sm" && "ow-button--sm",
    danger && "ow-button--danger",
    className,
  ]
    .filter(Boolean)
    .join(" ");

  const button = (
    <button
      {...rest}
      type={type}
      className={classes}
      aria-disabled={disabled || undefined}
      aria-busy={busy || undefined}
      onClick={guard(disabled || busy, onClick)}
    >
      {busy ? <Spinner /> : icon !== undefined && <Icon name={icon} size={size === "sm" ? "sm" : "md"} />}
      <span className="ow-button__label">{children}</span>
    </button>
  );
  return disabled ? <Tooltip text={disabledReason}>{button}</Tooltip> : button;
}

interface IconButtonProps extends Omit<NativeProps, "children" | "aria-label"> {
  icon: IconName;
  /** The name, also shown in the tooltip. */
  label: string;
  /** The keyboard shortcut, shown in the tooltip as "Hide (H)". */
  shortcut?: string;
  /** For a toggle: whether it is on. Leave out for a plain button. */
  pressed?: boolean;
  disabledReason?: string;
  busy?: boolean;
}

/** A 28 px button that shows only an icon. Every one has a tooltip. */
export function IconButton({
  icon,
  label,
  shortcut,
  pressed,
  disabledReason,
  busy = false,
  className,
  onClick,
  ...rest
}: IconButtonProps) {
  const disabled = disabledReason !== undefined;
  const classes = ["ow-icon-button", pressed === true && "ow-icon-button--on", className].filter(Boolean).join(" ");
  const tip = disabled
    ? disabledReason
    : shortcut === undefined
      ? label
      : controlsText.tooltipWithShortcut(label, shortcut);
  return (
    <Tooltip text={tip} describe={disabled}>
      <button
        {...rest}
        type="button"
        className={classes}
        aria-label={label}
        aria-keyshortcuts={shortcut}
        aria-pressed={pressed}
        aria-disabled={disabled || undefined}
        aria-busy={busy || undefined}
        onClick={guard(disabled || busy, onClick)}
      >
        {busy ? <Spinner /> : <Icon name={icon} />}
      </button>
    </Tooltip>
  );
}
