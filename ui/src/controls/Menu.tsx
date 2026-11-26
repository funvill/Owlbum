import { useEffect, useId, useLayoutEffect, useRef, useState, type KeyboardEvent } from "react";
import { createPortal } from "react-dom";
import { Icon, type IconName } from "../icons";
import { IconButton } from "./Button";
import { layerRoot } from "./layers";
import { placeWithin, viewSize, type Point } from "./place";
import { Tooltip } from "./Tooltip";
import "./Menu.css";

export { placeWithin } from "./place";

export interface MenuItem {
  id: string;
  label: string;
  icon?: IconName;
  /** Shown in the muted colour at the right, such as "Ctrl+C". */
  shortcut?: string;
  /** For a row that toggles: whether it is on. */
  checked?: boolean;
  /** Deletes: drawn with the error colour. */
  danger?: boolean;
  /** Why the row cannot be used now; it is then disabled. */
  disabledReason?: string;
  onSelect: () => void;
}

/** A row, or a rule between groups of rows. */
export type MenuEntry = MenuItem | "separator";

interface MenuProps {
  /** The name of the menu, usually the label of what opened it. */
  label: string;
  /** Where it opens: where the click was, or under the button. */
  at: Point;
  entries: MenuEntry[];
  onClose: () => void;
  /** Gets focus back when the menu closes with Escape or a choice. */
  returnFocus?: HTMLElement | null;
  /** A click here does not count as a click outside (the button that opened it). */
  anchor?: HTMLElement | null;
}

/**
 * A menu in the layer root: rows with an icon, a label and a shortcut,
 * moved through with the arrow keys, Home and End, chosen with Enter, Space
 * or a click, closed with Escape, Tab or a click outside. It opens where it
 * is asked to and moves to stay inside the window.
 * docs/09-visual-design.md, "Menus".
 */
export function Menu({ label, at, entries, onClose, returnFocus, anchor }: MenuProps) {
  const id = useId();
  const ref = useRef<HTMLDivElement>(null);
  const [place, setPlace] = useState<Point>(at);
  const items = entries.filter((e): e is MenuItem => e !== "separator");
  const reasonId = (item: MenuItem) => `${id}-${item.id}-reason`;

  const rows = () => [...(ref.current?.querySelectorAll<HTMLElement>("[data-menu-row]") ?? [])];

  const close = (refocus: boolean) => {
    onClose();
    if (refocus) returnFocus?.focus();
  };

  useLayoutEffect(() => {
    const menu = ref.current;
    if (menu === null) return;
    setPlace(placeWithin(at, { width: menu.offsetWidth, height: menu.offsetHeight }, viewSize()));
    rows()[0]?.focus();
  }, [at]);

  // A press anywhere outside the menu closes it.
  useEffect(() => {
    const onPointerDown = (e: PointerEvent) => {
      const target = e.target as Node;
      if (ref.current?.contains(target) || anchor?.contains(target)) return;
      onClose();
    };
    document.addEventListener("pointerdown", onPointerDown, true);
    return () => document.removeEventListener("pointerdown", onPointerDown, true);
  }, [onClose, anchor]);

  const choose = (item: MenuItem) => {
    if (item.disabledReason !== undefined) return;
    close(true);
    item.onSelect();
  };

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    const all = rows();
    const now = all.indexOf(document.activeElement as HTMLElement);
    const go = (i: number) => all[(i + all.length) % all.length]?.focus();
    switch (e.key) {
      case "ArrowDown":
        go(now + 1);
        break;
      case "ArrowUp":
        go(now - 1);
        break;
      case "Home":
        go(0);
        break;
      case "End":
        go(all.length - 1);
        break;
      case "Escape":
        e.stopPropagation();
        close(true);
        break;
      case "Tab":
        close(true);
        break;
      case "Enter":
      case " ": {
        const item = items.find((i) => i.id === all[now]?.dataset.menuRow);
        if (item !== undefined) choose(item);
        break;
      }
      default:
        return;
    }
    e.preventDefault();
  };

  return createPortal(
    <>
      <div
        ref={ref}
        role="menu"
        aria-label={label}
        className="ow-menu"
        style={{ left: place.x, top: place.y }}
        onKeyDown={onKeyDown}
      >
        {entries.map((entry, i) => {
          if (entry === "separator") return <div key={`separator-${i}`} role="separator" className="ow-menu__rule" />;
          const disabled = entry.disabledReason !== undefined;
          const toggles = entry.checked !== undefined;
          const row = (
            <div
              key={entry.id}
              data-menu-row={entry.id}
              role={toggles ? "menuitemcheckbox" : "menuitem"}
              aria-checked={toggles ? entry.checked : undefined}
              aria-disabled={disabled || undefined}
              aria-describedby={disabled ? reasonId(entry) : undefined}
              aria-keyshortcuts={entry.shortcut}
              tabIndex={-1}
              className={["ow-menu__row", entry.danger === true && "ow-menu__row--danger"].filter(Boolean).join(" ")}
              onClick={() => choose(entry)}
              onPointerMove={(e) => e.currentTarget.focus()}
            >
              <span className="ow-menu__icon">
                {toggles ? entry.checked && <Icon name="check" /> : entry.icon && <Icon name={entry.icon} />}
              </span>
              <span className="ow-menu__label">{entry.label}</span>
              {entry.shortcut !== undefined && (
                <span className="ow-menu__shortcut" aria-hidden="true">
                  {entry.shortcut}
                </span>
              )}
            </div>
          );
          return disabled ? (
            <Tooltip key={entry.id} text={entry.disabledReason!} describe={false}>
              {row}
            </Tooltip>
          ) : (
            row
          );
        })}
      </div>
      <div className="ow-visually-hidden">
        {items
          .filter((item) => item.disabledReason !== undefined)
          .map((item) => (
            <span key={item.id} id={reasonId(item)}>
              {item.disabledReason}
            </span>
          ))}
      </div>
    </>,
    layerRoot(),
  );
}

interface MenuButtonProps {
  label: string;
  icon: IconName;
  entries: MenuEntry[];
  shortcut?: string;
}

/** An icon button that opens a menu under itself. */
export function MenuButton({ label, icon, entries, shortcut }: MenuButtonProps) {
  const [open, setOpen] = useState<{ at: Point; button: HTMLElement } | null>(null);
  return (
    <>
      <IconButton
        icon={icon}
        label={label}
        shortcut={shortcut}
        aria-haspopup="menu"
        aria-expanded={open !== null}
        onClick={(e) => {
          if (open !== null) {
            setOpen(null);
            return;
          }
          const button = e.currentTarget;
          const rect = button.getBoundingClientRect();
          setOpen({ at: { x: rect.left, y: rect.bottom }, button });
        }}
      />
      {open !== null && (
        <Menu
          label={label}
          at={open.at}
          entries={entries}
          returnFocus={open.button}
          anchor={open.button}
          onClose={() => setOpen(null)}
        />
      )}
    </>
  );
}
