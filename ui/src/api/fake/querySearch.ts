// The fake query engine's search box and text search, as Rust runs them
// (owlbum_core::query::parse and owlbum_core::query::search;
// docs/05-organising.md, "Query words in the search box" and "Text search").
// The page itself holds no table of keys: only this fake and Rust do.

import type { Filter } from "../bindings/Filter";
import type { ParsedBox } from "../bindings/ParsedBox";
import type { TagFilter } from "../bindings/TagFilter";
import type { FakePhoto } from "./query";
import { folderOf } from "./queryFilter";

/** The fewest letters or digits search text needs to be a filter. */
const SEARCH_MIN = 2;

export function isSearch(search: string | null | undefined): boolean {
  return (search?.match(/[\p{L}\p{N}]/gu)?.length ?? 0) >= SEARCH_MIN;
}

/** Splits on spaces outside double quotes; each `"` switches quoting. */
function words(text: string): string[] {
  const out: string[] = [];
  let quoted = false;
  let word = "";
  for (const c of text) {
    if (c === '"') quoted = !quoted;
    if (c === " " && !quoted) {
      if (word !== "") out.push(word);
      word = "";
    } else word += c;
  }
  if (word !== "") out.push(word);
  return out;
}

/** Lower case without accents, split into runs of letters and digits. */
function fold(text: string): string[] {
  return text
    .normalize("NFD")
    .replace(/\p{M}/gu, "")
    .toLowerCase()
    .split(/[^\p{L}\p{N}]+/u)
    .filter((w) => w !== "");
}

/** True when every term is in one of `p`'s columns, as a phrase whose last word is a prefix. */
function matches(p: FakePhoto, terms: string[][]): boolean {
  const name = p.rel_path.slice(p.rel_path.lastIndexOf("/") + 1);
  const columns = [
    name,
    (p.people ?? []).join(" "),
    (p.keywords ?? []).join(" "),
    (p.objects ?? []).join(" "),
  ].map(fold);
  return terms.every((term) =>
    columns.some((ws) =>
      ws.some((_, i) => {
        const n = term.length - 1;
        return (
          i + n < ws.length &&
          term.slice(0, n).every((t, k) => ws[i + k] === t) &&
          ws[i + n]!.startsWith(term[n]!)
        );
      }),
    ),
  );
}

/** The ids of the photos the search text matches; null when it is not a filter. */
export function textMatches(
  photos: FakePhoto[],
  search: string | null,
): Set<number> | null {
  if (search === null || !isSearch(search)) return null;
  const terms = words(search)
    .map(fold)
    .filter((t) => t.length > 0);
  return new Set(photos.filter((p) => matches(p, terms)).map((p) => p.id));
}

const KEYS = [
  "star",
  "shape",
  "hidden",
  "from",
  "to",
  "folder",
  "has",
  "is",
  "tag",
  "object",
  "person",
];

function split(word: string): [string, string] | null {
  const at = word.indexOf(":");
  if (at < 1) return null;
  const key = word.slice(0, at).toLowerCase();
  if (!/^[a-z]+$/.test(key) || !KEYS.includes(key)) return null;
  let value = word.slice(at + 1);
  if (value.startsWith('"')) {
    value = value.slice(1);
    if (value.endsWith('"')) value = value.slice(0, -1);
  }
  return [key, value];
}

function isDay(day: string): boolean {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(day)) return false;
  const d = new Date(`${day}T00:00:00Z`);
  return !Number.isNaN(d.getTime()) && d.toISOString().slice(0, 10) === day;
}

/** A bad value: the hint for the message. */
class Bad {
  constructor(readonly hint: string) {}
}

/** `query_parse` over the fake album's folders and people. */
export function parseBox(
  photos: FakePhoto[],
  text: string,
  enter: boolean,
  filter: Filter,
): ParsedBox {
  const all = words(text);
  const quoted = (text.match(/"/g)?.length ?? 0) % 2 === 1;
  const unfinished =
    !enter && all.length > 0 && (quoted || !text.endsWith(" "))
      ? all[all.length - 1]!
      : null;
  const finished = unfinished === null ? all : all.slice(0, -1);
  const out: ParsedBox = {
    taken: false,
    set: {},
    add_tags: [],
    add_people: [],
    rest: "",
    search: null,
    error: null,
  };
  const set = out.set;
  const fixed = (options: string[], value: string, hint: string): number => {
    const i = options.indexOf(value.toLowerCase());
    if (i < 0) throw new Bad(hint);
    return i;
  };
  const apply = (key: string, value: string) => {
    switch (key) {
      case "star":
        set.star =
          fixed(["yes", "no"], value, "try star:yes or star:no") === 0
            ? "yes"
            : "no";
        return;
      case "shape":
        set.shape =
          fixed(
            ["portrait", "landscape"],
            value,
            "try shape:portrait or shape:landscape",
          ) === 0
            ? "portrait"
            : "landscape";
        return;
      case "hidden":
        set.hidden =
          fixed(["show", "only"], value, "try hidden:show or hidden:only") === 0
            ? "show"
            : "only";
        return;
      case "from":
      case "to": {
        if (!isDay(value)) throw new Bad(`try ${key}:YYYY-MM-DD`);
        const other =
          key === "from" ? (set.to ?? filter.to) : (set.from ?? filter.from);
        if (
          other !== undefined &&
          (key === "from" ? value > other : value < other)
        ) {
          throw new Bad("from must not be after to");
        }
        set[key] = value;
        return;
      }
      case "folder": {
        const folders = [...new Set(photos.map((p) => folderOf(p.rel_path)))];
        const found =
          value.toLowerCase() === "top"
            ? folders.find((f) => f === "")
            : folders.find((f) => f.toLowerCase() === value.toLowerCase());
        if (found === undefined) throw new Bad(`no folder named ${value}`);
        set.folder = found;
        return;
      }
      case "has":
        if (
          fixed(
            ["location", "people"],
            value,
            "try has:location or has:people",
          ) === 0
        )
          set.has_location = true;
        else set.has_people = true;
        return;
      case "is":
        if (
          fixed(
            ["lowquality", "duplicate"],
            value,
            "try is:lowquality or is:duplicate",
          ) === 0
        ) {
          set.low_quality = true;
        } else set.near_duplicates = true;
        return;
      case "tag":
      case "object": {
        const v = value.trim();
        if (v === "") throw new Bad(`try ${key}:<word>`);
        const kind: TagFilter["kind"] = key === "tag" ? "keyword" : "object";
        const had = [...(filter.tags ?? []), ...out.add_tags].some(
          (t) =>
            t.kind === kind && t.value.trim().toLowerCase() === v.toLowerCase(),
        );
        if (!had) out.add_tags.push({ kind, value: v });
        return;
      }
      default: {
        const v = value.trim();
        const name = [...new Set(photos.flatMap((p) => p.people ?? []))].find(
          (n) => v !== "" && n.toLowerCase() === v.toLowerCase(),
        );
        if (name === undefined) throw new Bad(`no person named ${v}`);
        const had = [...(filter.people ?? []), ...out.add_people].some(
          (n) => n.toLowerCase() === v.toLowerCase(),
        );
        if (!had) out.add_people.push(name);
      }
    }
  };
  const kept: string[] = [];
  for (const word of finished) {
    const kv = split(word);
    if (kv === null) {
      kept.push(word);
      continue;
    }
    try {
      apply(kv[0], kv[1]);
      out.taken = true;
    } catch (e) {
      if (!(e instanceof Bad)) throw e;
      kept.push(word);
      out.error ??= `"${word}" isn't a filter: ${e.hint}`;
    }
  }
  const tail =
    unfinished !== null && split(unfinished) === null ? [unfinished] : [];
  const search = [...kept, ...tail].join(" ").trim();
  out.search = search === "" ? null : search;
  let rest = kept.join(" ");
  if (unfinished !== null)
    rest = rest === "" ? unfinished : `${rest} ${unfinished}`;
  else if (!enter && rest !== "") rest += " ";
  out.rest = enter ? rest.trim() : rest.trimStart();
  return out;
}
