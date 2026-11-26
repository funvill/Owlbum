import table from "./keymap.json";

/**
 * The one table of keys (docs/08-desktop-and-release.md, "The key table").
 * The page's key handlers, the shortcut overlay and the shell's menu all
 * read `keymap.json`; nothing else defines a key.
 */

/** Where a row applies. `browse` is the grid, timeline, map and preview. */
export type Place = "everywhere" | "browse" | "viewer" | "edit";

/** Every id in `keymap.json`, in file order. A test keeps the two equal. */
export const KEY_IDS = [
  "app.shortcuts",
  "app.about",
  "app.feedback",
  "app.open_folder",
  "app.full_screen",
  "app.quit",
  "app.close_folder",
  "app.reload",
  "app.developer_tools",
  "app.next_region",
  "app.previous_region",
  "browse.previous",
  "browse.next",
  "browse.extend_previous",
  "browse.extend_next",
  "browse.first",
  "browse.last",
  "browse.open_viewer",
  "browse.escape",
  "browse.select_all",
  "browse.select_none",
  "browse.star",
  "browse.hide",
  "browse.score_up",
  "browse.score_down",
  "browse.undo",
  "browse.edit",
  "browse.rotate_left",
  "browse.rotate_right",
  "browse.before",
  "browse.copy",
  "browse.trash",
  "viewer.previous",
  "viewer.next",
  "viewer.first",
  "viewer.last",
  "viewer.close",
  "viewer.escape",
  "viewer.play",
  "viewer.details",
  "viewer.zoom_in",
  "viewer.zoom_out",
  "viewer.fit",
  "viewer.star",
  "viewer.hide",
  "viewer.score_up",
  "viewer.score_down",
  "viewer.undo",
  "viewer.rotate_left",
  "viewer.rotate_right",
  "viewer.before",
  "viewer.copy",
  "edit.done",
  "edit.cancel",
  "edit.rotate_left",
  "edit.rotate_right",
  "edit.before",
] as const;

/** The id of a row, such as `browse.star`. */
export type KeyId = (typeof KEY_IDS)[number];

/** One row of the table, with every flag filled in. */
export interface KeyRow {
  readonly id: KeyId;
  readonly place: Place;
  /**
   * One or more keys, the first being the one hints show. Each is a
   * `KeyboardEvent.key` value (upper case for a letter, `Space` for the
   * space bar) with `Ctrl+`, `Shift+` and `Alt+` in front, in that order.
   */
  readonly keys: readonly string[];
  /** The action lasts only while the key is down. */
  readonly hold: boolean;
  /** Holding the key repeats the action. */
  readonly repeat: boolean;
  /** A native menu accelerator: the shell binds it, never the page. */
  readonly menu: boolean;
  /** On the menu in development builds only. */
  readonly dev: boolean;
  /** Also acts while typing in a text field (not in a dialog). */
  readonly typing: boolean;
}

/** A table that breaks the rules of docs/08. */
export class KeymapError extends Error {
  override name = "KeymapError";
}

const PLACES: readonly Place[] = ["everywhere", "browse", "viewer", "edit"];
const FLAGS = ["hold", "repeat", "menu", "dev", "typing"] as const;
const FIELDS = new Set<string>(["id", "place", "keys", ...FLAGS]);
const MODIFIERS = ["Ctrl", "Shift", "Alt"] as const;

/** Named keys a row may use, spelled as `KeyboardEvent.key` (Space aside). */
export const NAMED_KEYS: ReadonlySet<string> = new Set([
  "Enter",
  "Space",
  "Escape",
  "Delete",
  "Backspace",
  "Tab",
  "Home",
  "End",
  "PageUp",
  "PageDown",
  "ArrowLeft",
  "ArrowRight",
  "ArrowUp",
  "ArrowDown",
  ...Array.from({ length: 24 }, (_, i) => `F${i + 1}`),
]);

/** A key such as `Ctrl+Shift+I`, split into its modifiers and its key. */
export interface KeySpec {
  readonly ctrl: boolean;
  readonly shift: boolean;
  readonly alt: boolean;
  /** `KeyboardEvent.key`, upper case for a letter, `Space` for the space bar. */
  readonly key: string;
}

/**
 * Splits a key from the table. Modifiers come first, in the order Ctrl,
 * Shift, Alt; the key itself may be `+`. Throws on anything else.
 */
export function parseKey(text: string): KeySpec {
  const found = { Ctrl: false, Shift: false, Alt: false };
  let rest = text;
  let next = 0;
  for (;;) {
    const m = /^([A-Za-z]+)\+(.+)$/.exec(rest);
    if (!m) break;
    const name = m[1] as (typeof MODIFIERS)[number];
    const index = MODIFIERS.indexOf(name);
    if (index < next) {
      throw new KeymapError(`"${text}": "${m[1]}" is not Ctrl, Shift or Alt in that order`);
    }
    found[name] = true;
    next = index + 1;
    rest = m[2] ?? "";
  }
  const isChar = [...rest].length === 1;
  if (isChar && /[a-z]/.test(rest)) {
    throw new KeymapError(`"${text}": write a letter in upper case`);
  }
  if (!isChar && !NAMED_KEYS.has(rest)) {
    throw new KeymapError(`"${text}": "${rest}" is not a key this table knows`);
  }
  if (isChar && rest === " ") throw new KeymapError(`"${text}": write the space bar as Space`);
  return { ctrl: found.Ctrl, shift: found.Shift, alt: found.Alt, key: rest };
}

/**
 * Checks a parsed `keymap.json` and fills in its flags. Throws a
 * `KeymapError` naming the first broken rule.
 */
export function parseKeymap(raw: unknown): KeyRow[] {
  if (!Array.isArray(raw)) throw new KeymapError("the key table is not a list of rows");
  const rows: KeyRow[] = [];
  const ids = new Set<string>();
  for (const [i, value] of raw.entries()) {
    if (typeof value !== "object" || value === null || Array.isArray(value)) {
      throw new KeymapError(`row ${i} is not an object`);
    }
    const r = value as Record<string, unknown>;
    const where = typeof r.id === "string" ? r.id : `row ${i}`;
    for (const field of Object.keys(r)) {
      if (!FIELDS.has(field)) throw new KeymapError(`${where}: unknown field "${field}"`);
    }
    if (typeof r.id !== "string" || !/^[a-z]+\.[a-z_]+$/.test(r.id)) {
      throw new KeymapError(`${where}: the id must look like "browse.star"`);
    }
    if (ids.has(r.id)) throw new KeymapError(`${where}: the id is used twice`);
    ids.add(r.id);
    if (!PLACES.includes(r.place as Place)) {
      throw new KeymapError(`${where}: place must be one of ${PLACES.join(", ")}`);
    }
    if (!Array.isArray(r.keys) || r.keys.length === 0 || r.keys.some((k) => typeof k !== "string")) {
      throw new KeymapError(`${where}: keys must be a list of one or more keys`);
    }
    for (const k of r.keys as string[]) parseKey(k);
    const flags = {} as Record<(typeof FLAGS)[number], boolean>;
    for (const f of FLAGS) {
      if (r[f] !== undefined && typeof r[f] !== "boolean") {
        throw new KeymapError(`${where}: ${f} must be true or false`);
      }
      flags[f] = r[f] === true;
    }
    if (flags.menu && r.keys.length !== 1) {
      throw new KeymapError(`${where}: a menu accelerator has exactly one key`);
    }
    if (flags.dev && !flags.menu) {
      throw new KeymapError(`${where}: only a menu row can be for development builds`);
    }
    if (flags.hold && flags.repeat) {
      throw new KeymapError(`${where}: a row cannot both hold and repeat`);
    }
    rows.push({
      id: r.id as KeyId,
      place: r.place as Place,
      keys: [...(r.keys as string[])],
      ...flags,
    });
  }
  checkClashes(rows);
  return rows;
}

/** No key may reach two rows at once: in one place, or in a place and everywhere. */
function checkClashes(rows: readonly KeyRow[]) {
  for (const place of PLACES) {
    if (place === "everywhere") continue;
    const seen = new Map<string, KeyId>();
    for (const r of rows) {
      if (r.place !== place && r.place !== "everywhere") continue;
      for (const k of r.keys) {
        const other = seen.get(canonical(k));
        if (other && other !== r.id) {
          throw new KeymapError(`${r.id}: "${k}" already belongs to ${other} in ${place}`);
        }
        seen.set(canonical(k), r.id);
      }
    }
  }
}

function canonical(text: string): string {
  const s = parseKey(text);
  return `${s.ctrl ? "Ctrl+" : ""}${s.shift ? "Shift+" : ""}${s.alt ? "Alt+" : ""}${s.key}`;
}

/** The table, checked once when the page loads. */
export const KEYMAP: readonly KeyRow[] = parseKeymap(table);

const BY_ID = new Map<KeyId, KeyRow>(KEYMAP.map((r) => [r.id, r]));

/** The row with this id. */
export function row(id: KeyId): KeyRow {
  const r = BY_ID.get(id);
  if (!r) throw new KeymapError(`no row ${id}`);
  return r;
}

/** The key hints show for an action, such as `F` for `browse.star`. */
export function primaryKey(id: KeyId): string {
  return row(id).keys[0] ?? "";
}

/**
 * The rows that apply in a place: its own and the `everywhere` rows.
 * Development-only rows are left out unless `dev` is set.
 */
export function rowsFor(
  place: Exclude<Place, "everywhere">,
  { dev = false }: { dev?: boolean } = {},
): KeyRow[] {
  return KEYMAP.filter(
    (r) => (r.place === place || r.place === "everywhere") && (dev || !r.dev),
  );
}

/**
 * Compares a set of key handlers with the rows they serve (by default every
 * row the page handles: all but the menu accelerators). `missing` are rows
 * with no handler; `unknown` are handlers for ids that are not rows.
 * Page tests use it so a handler and the table never drift apart.
 */
export function handlerGaps(
  handlers: Readonly<Record<string, unknown>>,
  rows: readonly KeyRow[] = KEYMAP.filter((r) => !r.menu),
): { missing: KeyId[]; unknown: string[] } {
  const wanted = new Set<string>(rows.map((r) => r.id));
  return {
    missing: rows.filter((r) => !(r.id in handlers)).map((r) => r.id),
    unknown: Object.keys(handlers).filter((id) => !wanted.has(id)),
  };
}
