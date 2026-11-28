// The rules of the typed fields (docs/04-browsing.md, "How typed fields
// save"; docs/05-organising.md, "People suggestions"). Pure functions, so
// each row of the spec's tables is tested on its own. Rust checks again.

import type { PersonCount } from "../../../api/bindings/PersonCount";
import type { PhotoDetail } from "../../../api/bindings/PhotoDetail";
import type { TypedLocation } from "../../../api/bindings/TypedLocation";
import { detailsText as t } from "../../../text/details";

/** The Location group's three boxes, as typed. */
export interface LocationBoxes {
  lat: string;
  lon: string;
  name: string;
}

/** The longest name, in characters (as in Rust). */
export const MAX_NAME_CHARS = 100;

/** At most this many suggestions show. */
export const MAX_SUGGESTIONS = 8;

/** A text field's value as sent: trimmed, or null when that leaves nothing. */
export function cleanText(text: string): string | null {
  const s = text.trim();
  return s === "" ? null : s;
}

/** A person's name: trimmed, inner spaces made one, at most 100 characters; null when empty. */
export function cleanName(name: string): string | null {
  const joined = name.trim().split(/\s+/).join(" ");
  const cut = Array.from(joined).slice(0, MAX_NAME_CHARS).join("").trimEnd();
  return cut === "" ? null : cut;
}

/** Lower case without accents, for comparing names and matching suggestions. */
export function fold(text: string): string {
  return text.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase();
}

/**
 * The photo's names after adding `typed` names: each cleaned, the album's
 * spelling used for a name it knows (ignoring case), and a name already on
 * the photo not added twice.
 */
export function addNames(current: string[], typed: string[], known: PersonCount[]): string[] {
  const out = [...current];
  const have = new Set(out.map((n) => n.toLowerCase()));
  for (const raw of typed) {
    const name = cleanName(raw);
    if (name === null) continue;
    const key = name.toLowerCase();
    if (have.has(key)) continue;
    const stored = known.find((k) => k.name.toLowerCase() === key)?.name;
    out.push(stored ?? name);
    have.add(key);
  }
  return out;
}

/**
 * Splits what is in the People box at commas: the complete names before the
 * last comma are to be added, and the text after it stays in the box.
 */
export function splitAtCommas(text: string): { names: string[]; rest: string } {
  const parts = text.split(",");
  const rest = parts.pop() ?? "";
  return { names: parts, rest };
}

/**
 * The suggestions to show: the album's names not on the photo, narrowed to
 * names with a word that starts with the typed text (ignoring case and
 * accents), most used first (the order `album_people` gives), at most 8.
 */
export function suggestionsFor(known: PersonCount[], onPhoto: string[], typed: string): string[] {
  const have = new Set(onPhoto.map((n) => n.toLowerCase()));
  const q = fold(typed.trim());
  return known
    .filter((k) => !have.has(k.name.toLowerCase()))
    .filter(
      (k) =>
        q === "" ||
        fold(k.name)
          .split(/\s+/)
          .some((w) => w.startsWith(q)),
    )
    .slice(0, MAX_SUGGESTIONS)
    .map((k) => k.name);
}

const pad = (n: number) => String(n).padStart(2, "0");

function daysIn(y: number, m: number): number {
  return new Date(Date.UTC(y, m, 0)).getUTCDate();
}

/**
 * A typed date as `YYYY-MM-DDTHH:MM:SS`, from `YYYY-MM-DD HH:MM:SS`,
 * `YYYY-MM-DD HH:MM` or `YYYY-MM-DD` with a space or a `T`; null when it
 * is not a real date and time with a year from 1800 to 2200. No time zone.
 */
export function parseDate(typed: string): string | null {
  const m = /^(\d{4})-(\d{2})-(\d{2})(?:[ T](\d{2}):(\d{2})(?::(\d{2}))?)?$/.exec(typed.trim());
  if (m === null) return null;
  const [y, mo, d] = [Number(m[1]), Number(m[2]), Number(m[3])];
  const [h, mi, s] = m[4] === undefined ? [12, 0, 0] : [Number(m[4]), Number(m[5]), Number(m[6] ?? 0)];
  const ok =
    y >= 1800 && y <= 2200 && mo >= 1 && mo <= 12 && d >= 1 && d <= daysIn(y, mo) && h < 24 && mi < 60 && s < 60;
  return ok ? `${m[1]}-${m[2]}-${m[3]}T${pad(h)}:${pad(mi)}:${pad(s)}` : null;
}

/** What the Date box shows: the effective date as `YYYY-MM-DD HH:MM:SS`, or "". */
export function shownDate(detail: PhotoDetail): string {
  return detail.date.effective?.replace("T", " ") ?? "";
}

/** What leaving the Date box does. */
export type DateOutcome =
  { kind: "none" } | { kind: "restore" } | { kind: "send"; value: string | null } | { kind: "error"; message: string };

/** The date table: what to do with `typed` for this photo. */
export function dateOutcome(typed: string, detail: PhotoDetail): DateOutcome {
  const text = typed.trim();
  if (text === shownDate(detail)) return { kind: "none" };
  if (text === "") return detail.date.typed !== null ? { kind: "send", value: null } : { kind: "restore" };
  const value = parseDate(text);
  if (value === null) return { kind: "error", message: t.invalidDate };
  if (value === detail.date.camera) {
    return detail.date.typed !== null ? { kind: "send", value: null } : { kind: "restore" };
  }
  if (value === detail.date.effective) return { kind: "restore" };
  return { kind: "send", value };
}

/** The boxes for a photo: its effective place and the typed place name. */
export function boxesFor(detail: PhotoDetail): LocationBoxes {
  const { lat, lon, name } = detail.place;
  return {
    lat: lat === null ? "" : String(lat),
    lon: lon === null ? "" : String(lon),
    name: name ?? "",
  };
}

/** The camera's place in the boxes' form. */
function cameraBoxes(detail: PhotoDetail): LocationBoxes {
  const gps = detail.place.gps;
  return {
    lat: gps === null ? "" : String(gps.lat),
    lon: gps === null ? "" : String(gps.lon),
    name: "",
  };
}

/** What leaving the Location group does. */
export type LocationOutcome =
  | { kind: "none" }
  | { kind: "restore"; boxes: LocationBoxes }
  | { kind: "send"; location: TypedLocation | null }
  | { kind: "error"; message: string };

/** A coordinate box: null when empty, NaN when not a number. A decimal comma is accepted. */
function coordinate(text: string): number | null {
  const s = text.trim().replace(",", ".");
  if (s === "") return null;
  return /^[-+]?(\d+\.?\d*|\.\d+)$/.test(s) ? Number(s) : NaN;
}

const same5 = (a: number, b: number) => Math.round(a * 1e5) === Math.round(b * 1e5);

/** The location table: what to do with the boxes for this photo. */
export function locationOutcome(boxes: LocationBoxes, detail: PhotoDetail): LocationOutcome {
  const lat = coordinate(boxes.lat);
  const lon = coordinate(boxes.lon);
  const name = cleanText(boxes.name);
  const typed = detail.place.typed;
  if (lat !== null && Number.isNaN(lat)) return { kind: "error", message: t.latNumber };
  if (lon !== null && Number.isNaN(lon)) return { kind: "error", message: t.lonNumber };
  if (lat !== null && (lat < -90 || lat > 90)) return { kind: "error", message: t.latRange };
  if (lon !== null && (lon < -180 || lon > 180)) return { kind: "error", message: t.lonRange };
  if ((lat === null) !== (lon === null)) return { kind: "error", message: t.bothNeeded };
  if (lat === null || lon === null) {
    if (name !== null) return { kind: "error", message: t.nameNeedsPlace };
    return typed ? { kind: "send", location: null } : { kind: "restore", boxes: cameraBoxes(detail) };
  }
  const gps = detail.place.gps;
  if (name === null && gps !== null && same5(lat, gps.lat) && same5(lon, gps.lon)) {
    return typed ? { kind: "send", location: null } : { kind: "none" };
  }
  const cur = detail.place;
  if (typed && cur.lat === lat && cur.lon === lon && cur.name === name) return { kind: "none" };
  return { kind: "send", location: { lat, lon, name } };
}
