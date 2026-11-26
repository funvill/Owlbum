import { useEffect, useId, useRef, type KeyboardEvent, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { controlsText } from "../text/controls";
import { IconButton } from "./Button";
import { focusableIn } from "./focus";
import { layerRoot } from "./layers";
import "./Dialog.css";

interface DialogProps {
  open: boolean;
  title: string;
  onClose: () => void;
  children: ReactNode;
  /** The row of buttons at the bottom right, primary last. */
  actions?: ReactNode;
  size?: "sm" | "wide";
  /** Only asks to confirm a deletion: an alert dialog. */
  alert?: boolean;
  /** Holds unsaved input: a click on the backdrop does not close it. */
  dirty?: boolean;
}

/**
 * A modal dialog in the layer root. Focus starts on the button marked
 * `data-autofocus` (the safest one), else on its first control. It keeps focus inside, makes the rest
 * of the page inert, closes on Escape, its x, or a click on the backdrop
 * (unless it holds unsaved input), and returns focus to what opened it.
 * docs/09-visual-design.md, "Dialogs" and "Accessibility baseline".
 */
export function Dialog(props: DialogProps) {
  if (!props.open) return null;
  return createPortal(<OpenDialog {...props} />, layerRoot());
}

function OpenDialog({ title, onClose, children, actions, size = "sm", alert = false, dirty = false }: DialogProps) {
  const titleId = useId();
  const box = useRef<HTMLDivElement>(null);
  const body = useRef<HTMLDivElement>(null);
  const pressedOnBackdrop = useRef(false);

  useEffect(() => {
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const root = layerRoot();
    const madeInert = [...document.body.children].filter(
      (el): el is HTMLElement => el instanceof HTMLElement && el !== root && !el.hasAttribute("inert"),
    );
    for (const el of madeInert) el.setAttribute("inert", "");

    // The safest button, when the dialog marks one with `data-autofocus`;
    // else the first control of the body, else of the box.
    const safest = box.current?.querySelector<HTMLElement>("[data-autofocus]") ?? null;
    const first =
      safest ?? (body.current && focusableIn(body.current)[0]) ?? (box.current && focusableIn(box.current)[0]);
    (first ?? box.current)?.focus();

    return () => {
      for (const el of madeInert) el.removeAttribute("inert");
      opener?.focus();
    };
  }, []);

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.key === "Escape") {
      e.stopPropagation();
      onClose();
      return;
    }
    if (e.key !== "Tab" || box.current === null) return;
    const items = focusableIn(box.current);
    const first = items[0];
    const last = items[items.length - 1];
    if (first === undefined || last === undefined) return;
    if (e.shiftKey && document.activeElement === first) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && document.activeElement === last) {
      e.preventDefault();
      first.focus();
    }
  };

  return (
    <div className="ow-dialog-layer">
      <div
        className="ow-dialog-backdrop"
        onPointerDown={(e) => {
          pressedOnBackdrop.current = e.target === e.currentTarget;
        }}
        onClick={(e) => {
          const onBackdrop = e.target === e.currentTarget && pressedOnBackdrop.current;
          pressedOnBackdrop.current = false;
          if (!onBackdrop) return;
          // With unsaved input it stays open, and keeps focus for Escape.
          if (dirty) box.current?.focus();
          else onClose();
        }}
      />
      <div
        ref={box}
        role={alert ? "alertdialog" : "dialog"}
        aria-modal="true"
        aria-labelledby={titleId}
        tabIndex={-1}
        className={`ow-dialog ow-dialog--${size}`}
        onKeyDown={onKeyDown}
      >
        <header className="ow-dialog__header">
          <h2 id={titleId} className="ow-dialog__title">
            {title}
          </h2>
          <IconButton icon="x" label={controlsText.close} onClick={onClose} />
        </header>
        <div ref={body} className="ow-dialog__body">
          {children}
        </div>
        {actions !== undefined && <footer className="ow-dialog__actions">{actions}</footer>}
      </div>
    </div>
  );
}
