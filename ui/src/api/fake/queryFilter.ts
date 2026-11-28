// The fake query engine's filters, chips and facets, as Rust runs them
// (owlbum_core::query::filter; docs/05-organising.md, "The list of filters").

import type { AppError } from "../bindings/AppError";
import type { Chip } from "../bindings/Chip";
import type { Facets } from "../bindings/Facets";
import type { Filter } from "../bindings/Filter";
import type { KindFilter } from "../bindings/KindFilter";
import { fakeFormats } from "./app";
import type { FakePhoto } from "./query";

const STAR = 1;
const SHAPE = 2;
const HIDDEN = 4;
const DATES = 8;
const FOLDER = 16;
const LOCATION = 32;
const HAS_PEOPLE = 64;
const CONTENT = 128;
const LOW_QUALITY = 256;
const NEAR_DUPLICATES = 512;
/** Search text: no group, so a photo failing it counts in no facet. */
const TEXT = 1024;
const KIND = 2048;
const FORMAT = 4096;

const KINDS: KindFilter[] = ["still", "animated", "raw", "video"];

/** The media type filter's kind of `p`, as Rust's `kind_of`. */
function kindOf(p: FakePhoto): KindFilter {
  if (p.kind === "raw" || p.kind === "video") return p.kind;
  return p.duration_ms !== undefined && p.duration_ms !== null ? "animated" : "still";
}

/** The registry id of `p`'s format: the fake registry's first when the photo names none. */
const formatOf = (p: FakePhoto) => p.format ?? fakeFormats[0]!.id;

/** The media type chip's text, as Rust's `chip::kinds`. */
function kindsText(kinds: KindFilter[]): string {
  const plural: Record<KindFilter, string> = { still: "Stills", animated: "Animated", raw: "RAW", video: "Videos" };
  const out = KINDS.filter((k) => !kinds.includes(k));
  if (out.length === 1) return out[0] === "raw" ? "No RAW" : `No ${plural[out[0]!].toLowerCase()}`;
  return kinds.map((k) => plural[k]).join(", ");
}

export function folderOf(relPath: string): string {
  const at = relPath.lastIndexOf("/");
  return at < 0 ? "" : relPath.slice(0, at);
}

/** The filter with every off value taken out, so "absent" has one form. */
export function normalised(filter: Filter): Filter {
  const out: Filter = {};
  for (const [key, value] of Object.entries(filter) as [keyof Filter, unknown][]) {
    if (value === undefined || value === null || value === false) continue;
    if (Array.isArray(value) && value.length === 0) continue;
    (out as Record<string, unknown>)[key] = value;
  }
  if (out.kinds !== undefined) {
    const kinds = KINDS.filter((k) => out.kinds!.includes(k));
    if (kinds.length === KINDS.length) delete out.kinds;
    else out.kinds = kinds;
  }
  return out;
}

function isDay(day: string): boolean {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(day)) return false;
  const d = new Date(`${day}T00:00:00Z`);
  return !Number.isNaN(d.getTime()) && d.toISOString().slice(0, 10) === day;
}

/** Throws `bad_query` for a value that is not allowed, with Rust's message. */
export function validate(filter: Filter): void {
  const bad = (value: string): never => {
    const e: AppError = {
      code: "bad_query",
      message: `Owlbum couldn't use "${value}" as a filter. Try clearing the filter.`,
    };
    throw e;
  };
  for (const day of [filter.from, filter.to]) if (day !== undefined && !isDay(day)) bad(day);
  for (const person of filter.people ?? []) if (person.trim() === "") bad(person);
  for (const tag of filter.tags ?? []) if (tag.value.trim() === "") bad(tag.value);
}

function shapeOf(p: FakePhoto): "portrait" | "landscape" | null {
  const w = p.width ?? 0;
  const h = p.height ?? 0;
  if (w === 0 || h === 0 || w === h) return null;
  return h > w ? "portrait" : "landscape";
}

const hasPeople = (p: FakePhoto) => (p.people ?? []).length > 0;

/** The filters `p` fails, one bit each. `filter` is normalised. */
function fails(p: FakePhoto, filter: Filter, text: Set<number> | null): number {
  let mask = 0;
  const fail = (bit: number, passes: boolean) => {
    if (!passes) mask |= bit;
  };
  const hidden = p.hidden ?? false;
  if (filter.star !== undefined) fail(STAR, (p.star ?? false) === (filter.star === "yes"));
  if (filter.shape !== undefined) fail(SHAPE, shapeOf(p) === filter.shape);
  if (filter.hidden === undefined) fail(HIDDEN, !hidden);
  else if (filter.hidden === "only") fail(HIDDEN, hidden);
  if (filter.from !== undefined || filter.to !== undefined) {
    const day = p.date?.slice(0, 10) ?? null;
    fail(DATES, day !== null && (filter.from ?? "") <= day && day <= (filter.to ?? "9999-99-99"));
  }
  if (filter.folder !== undefined) fail(FOLDER, folderOf(p.rel_path) === filter.folder);
  if (filter.has_location) fail(LOCATION, p.has_location ?? false);
  if (filter.has_people) fail(HAS_PEOPLE, hasPeople(p));
  const names = (p.people ?? []).map((n) => n.toLowerCase());
  const people = (filter.people ?? []).every((n) => names.includes(n.trim().toLowerCase()));
  const tags = (filter.tags ?? []).every((t) =>
    (t.kind === "keyword" ? (p.keywords ?? []) : (p.objects ?? []))
      .map((l) => l.toLowerCase())
      .includes(t.value.trim().toLowerCase()),
  );
  if (filter.people !== undefined || filter.tags !== undefined) fail(CONTENT, people && tags);
  if (filter.low_quality) fail(LOW_QUALITY, p.low_quality ?? false);
  if (filter.near_duplicates) fail(NEAR_DUPLICATES, p.burst ?? p.burst_id !== undefined);
  if (text !== null) fail(TEXT, text.has(p.id));
  if (filter.kinds !== undefined) fail(KIND, filter.kinds.includes(kindOf(p)));
  if (filter.formats !== undefined) fail(FORMAT, filter.formats.includes(formatOf(p)));
  return mask;
}

/**
 * What one pass gives: the photos that pass, Y, and the facets when asked for.
 * `text` holds the ids the search text matches; null without search text.
 */
export function pass(
  photos: FakePhoto[],
  filter: Filter,
  scope: string,
  wantFacets: boolean,
  text: Set<number> | null = null,
): { shown: FakePhoto[]; total: number; facets: Facets | undefined } {
  const shown: FakePhoto[] = [];
  let total = 0;
  const f: Facets = {
    star: { yes: 0, no: 0 },
    shape: { portrait: 0, landscape: 0 },
    hidden: 0,
    folders: [],
    has_location: 0,
    has_people: 0,
    low_quality: 0,
    near_duplicates: 0,
    kinds: { still: 0, animated: 0, raw: 0, video: 0 },
    formats: [],
  };
  const folders = new Map<string, number>();
  const formats = new Map<string, number>();
  for (const p of photos) {
    if (filter.hidden !== undefined || !(p.hidden ?? false)) total += 1;
    const mask = fails(p, filter, text);
    if (mask === 0) shown.push(p);
    if (!wantFacets) continue;
    const counts = (bit: number) => (mask & ~bit) === 0;
    const folder = folderOf(p.rel_path);
    folders.set(folder, (folders.get(folder) ?? 0) + (counts(FOLDER) ? 1 : 0));
    if (counts(STAR)) {
      if (p.star ?? false) f.star.yes += 1;
      else f.star.no += 1;
    }
    const shape = shapeOf(p);
    if (counts(SHAPE) && shape !== null) f.shape[shape] += 1;
    if (counts(HIDDEN) && (p.hidden ?? false)) f.hidden += 1;
    if (counts(LOCATION) && (p.has_location ?? false)) f.has_location += 1;
    if (counts(HAS_PEOPLE) && hasPeople(p)) f.has_people += 1;
    if (counts(LOW_QUALITY) && (p.low_quality ?? false)) f.low_quality += 1;
    if (counts(NEAR_DUPLICATES) && (p.burst ?? p.burst_id !== undefined)) f.near_duplicates += 1;
    if (counts(KIND)) f.kinds[kindOf(p)] += 1;
    formats.set(formatOf(p), (formats.get(formatOf(p)) ?? 0) + (counts(FORMAT) ? 1 : 0));
  }
  f.formats = fakeFormats
    .filter((x) => formats.has(x.id))
    .map((x) => ({ format: x.id, label: x.label, count: formats.get(x.id)! }));
  f.folders = [...folders]
    .map(([folder, count]) => ({ folder, label: folder === scope ? "Top folder" : folder, count }))
    .sort((a, b) => Number(a.folder !== scope) - Number(b.folder !== scope) || (a.folder < b.folder ? -1 : 1));
  return { shown, total, facets: wantFacets ? f : undefined };
}

/** One chip per filter that is on, in the order of the list. `filter` is normalised. */
export function chips(filter: Filter, scope: string): Chip[] {
  const out: Chip[] = [];
  const chip = (id: string, text: string, remove: (f: Filter) => void) => {
    const without: Filter = structuredClone(filter);
    remove(without);
    out.push({ id, text, remove: normalised(without) });
  };
  if (filter.star) chip("star", filter.star === "yes" ? "Starred" : "Not starred", (f) => delete f.star);
  if (filter.shape) chip("shape", filter.shape === "portrait" ? "Portrait" : "Landscape", (f) => delete f.shape);
  if (filter.hidden) chip("hidden", filter.hidden === "show" ? "Hidden shown" : "Only hidden", (f) => delete f.hidden);
  if (filter.from !== undefined || filter.to !== undefined) {
    const text =
      filter.from !== undefined && filter.to !== undefined
        ? `${filter.from} → ${filter.to}`
        : filter.from !== undefined
          ? `From ${filter.from}`
          : `Until ${filter.to}`;
    chip("dates", text, (f) => {
      delete f.from;
      delete f.to;
    });
  }
  if (filter.folder !== undefined) {
    chip("folder", filter.folder === scope ? "Top folder" : filter.folder, (f) => delete f.folder);
  }
  if (filter.has_location) chip("has_location", "Has location", (f) => delete f.has_location);
  if (filter.has_people) chip("has_people", "Has people", (f) => delete f.has_people);
  (filter.people ?? []).forEach((name, i) => chip(`person:${name}`, name, (f) => f.people?.splice(i, 1)));
  (filter.tags ?? []).forEach((t, i) => chip(`tag:${t.kind}:${t.value}`, t.value, (f) => f.tags?.splice(i, 1)));
  if (filter.low_quality) chip("low_quality", "Low quality", (f) => delete f.low_quality);
  if (filter.near_duplicates) chip("near_duplicates", "Near-duplicates", (f) => delete f.near_duplicates);
  if (filter.kinds !== undefined) chip("kinds", kindsText(filter.kinds), (f) => delete f.kinds);
  (filter.formats ?? []).forEach((id, i) =>
    chip(`format:${id}`, fakeFormats.find((x) => x.id === id)?.label ?? id, (f) => f.formats?.splice(i, 1)),
  );
  return out;
}
