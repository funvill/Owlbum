import { KEYMAP, parseKey, row } from "./table";
import type { KeyId, KeyRow, KeySpec, Place } from "./table";

/**
 * Key matching, the same for every row (docs/08-desktop-and-release.md,
 * "The key table"). Owlbum runs on Windows and Linux: `Ctrl+` in the table
 * is the Ctrl key, and Meta (the Windows or Super key) never matches.
 */

/** The parts of a `KeyboardEvent` matching reads. */
export interface KeyLike {
  key: string;
  code?: string;
  ctrlKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
  metaKey: boolean;
  repeat: boolean;
  isComposing?: boolean;
  defaultPrevented?: boolean;
  target?: EventTarget | null;
  getModifierState?(key: string): boolean;
}

/**
 * Where keyboard focus is, as matching sees it.
 *
 * - `typing`: a text field, a drop-down or an editable element.
 * - `dialog`: inside a dialog, or inside an element marked
 *   `data-keys="dialog"` (the Filters panel).
 * - `menu`: inside a menu, which owns its arrows, Home, End, Enter, Space
 *   and Esc.
 * - `control`: a button, link, checkbox or the like, which owns Enter and
 *   Space.
 * - `none`: anywhere else, sliders included.
 */
export type Focus = "none" | "typing" | "control" | "menu" | "dialog";

export interface MatchContext {
  /** The place open now: the viewer, edit mode, or else `browse`. */
  place: Exclude<Place, "everywhere">;
  /**
   * Overrides the focus read from the event's target. Pass `dialog` while
   * a dialog is open, wherever focus is.
   */
  focus?: Focus;
}

export interface KeyMatch {
  id: KeyId;
  row: KeyRow;
  /**
   * False for an auto-repeat of a row that does not repeat (or a hold row,
   * which starts once). The caller still owns the key (it should prevent
   * the default) but does not act.
   */
  act: boolean;
}

/** Keys a menu uses to move and choose; the table's rows do not get them. */
const MENU_KEYS = new Set([
  "ArrowUp",
  "ArrowDown",
  "ArrowLeft",
  "ArrowRight",
  "Home",
  "End",
  "Enter",
  "Space",
  "Escape",
  "Tab",
]);

/** Keys a focused control presses itself. */
const CONTROL_KEYS = new Set(["Enter", "Space"]);

const TEXT_INPUT_TYPES = new Set([
  "",
  "text",
  "search",
  "email",
  "number",
  "password",
  "tel",
  "url",
  "date",
  "datetime-local",
  "month",
  "time",
  "week",
]);

const CONTROL_ROLES = [
  "button",
  "link",
  "checkbox",
  "radio",
  "switch",
  "tab",
];

const CONTROL_SELECTOR = [
  "button",
  "a[href]",
  "summary",
  "input",
  ...CONTROL_ROLES.map((r) => `[role="${r}"]`),
].join(",");

/** Classifies where focus is. See `Focus`. */
export function focusOf(target: EventTarget | null | undefined): Focus {
  if (!target || !(target instanceof Element)) return "none";
  const layer = target.closest('dialog,[role="dialog"],[role="alertdialog"],[data-keys]');
  if (layer && layer.getAttribute("data-keys") !== "surface") return "dialog";
  if (isTextField(target)) return "typing";
  if (target.closest('[role="menu"],[role="menubar"]')) return "menu";
  if (target instanceof HTMLInputElement && target.type === "range") return "none";
  if (target.closest(CONTROL_SELECTOR)) return "control";
  return "none";
}

function isTextField(el: Element): boolean {
  if (el instanceof HTMLInputElement) return TEXT_INPUT_TYPES.has(el.type.toLowerCase());
  if (el instanceof HTMLTextAreaElement || el instanceof HTMLSelectElement) return true;
  const editable = el.closest("[contenteditable]");
  return !!editable && editable.getAttribute("contenteditable") !== "false";
}

/**
 * Finds the row a keydown reaches in this place, or null. Menu rows never
 * match: the shell's menu owns them.
 */
export function matchKey(event: KeyLike, context: MatchContext): KeyMatch | null {
  if (event.defaultPrevented || event.isComposing) return null;
  const focus = context.focus ?? focusOf(event.target);
  if (focus === "dialog") return null;
  for (const r of KEYMAP) {
    if (r.menu) continue;
    if (r.place !== context.place && r.place !== "everywhere") continue;
    if (focus === "typing" && !r.typing) continue;
    for (const text of r.keys) {
      const spec = parseKey(text);
      if (focus === "control" && CONTROL_KEYS.has(spec.key)) continue;
      if (focus === "menu" && MENU_KEYS.has(spec.key)) continue;
      if (specMatches(event, spec)) {
        return { id: r.id, row: r, act: !event.repeat || r.repeat };
      }
    }
  }
  return null;
}

/**
 * Whether a key event is one of this row's keys, with its modifiers. Place,
 * focus and repeat are not considered. The overlay uses it to close on `?`.
 */
export function keyMatches(event: KeyLike, id: KeyId): boolean {
  return row(id).keys.some((text) => specMatches(event, parseKey(text)));
}

/**
 * Whether a keyup releases a hold row's key. Modifiers and focus are not
 * considered, so a hold always ends.
 */
export function isRelease(event: KeyLike, id: KeyId): boolean {
  return row(id).keys.some((text) => sameKey(event, parseKey(text).key));
}

function specMatches(event: KeyLike, spec: KeySpec): boolean {
  if (event.metaKey) return false;
  if (!sameKey(event, spec.key)) return false;
  if (isLetter(spec.key)) return modifiersEqual(event, spec);
  if ([...spec.key].length === 1) {
    // A symbol matches on the character produced: Shift may have been
    // needed for it, and AltGr shows as Ctrl+Alt on Windows.
    const altGr = event.getModifierState?.("AltGraph") || (event.ctrlKey && event.altKey);
    const ctrl = altGr ? spec.ctrl : event.ctrlKey;
    const alt = altGr ? spec.alt : event.altKey;
    return ctrl === spec.ctrl && alt === spec.alt;
  }
  return modifiersEqual(event, spec);
}

function modifiersEqual(event: KeyLike, spec: KeySpec): boolean {
  return (
    event.ctrlKey === spec.ctrl && event.shiftKey === spec.shift && event.altKey === spec.alt
  );
}

function isLetter(key: string): boolean {
  return /^[A-Z]$/.test(key);
}

/** The key, spelled as the table spells it. */
function eventKey(event: KeyLike): string {
  if (event.key === " " || event.key === "Spacebar") return "Space";
  if (event.key === "Esc") return "Escape";
  return event.key;
}

function sameKey(event: KeyLike, key: string): boolean {
  const pressed = eventKey(event);
  if (!isLetter(key)) return pressed === key;
  if (pressed.toUpperCase() === key) return true;
  // Letters match in either case. On a layout of another alphabet, where
  // the key gives a letter that is not Latin, the physical key is used
  // instead. Punctuation and AltGr symbols (Dvorak's `.`, AltGr+E's `€`)
  // never fall back.
  if (/^\p{L}$/u.test(pressed) && !/\p{Script=Latin}/u.test(pressed)) {
    return event.code === `Key${key}`;
  }
  return false;
}
