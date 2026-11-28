// Every entry of the text modules, by key, so a translation can replace it
// (docs/09-visual-design.md, "Languages"; issue 76).
//
// A key is the module's file name, the exported object's name and the path
// inside it: "grid.gridText.label", "details.detailsText.sections.camera".
// An entry is either a string or a function that builds a string; a
// translation gives a string for either, a message (./message.ts) for a
// function. Every text module under ui/src/text is found by itself.

import { formatMessage, MessageError, messageArguments } from "./message";

/** A text entry as the English module holds it. */
export type Entry = string | ((...args: never[]) => unknown);

interface Slot {
  owner: Record<string, unknown>;
  field: string;
  english: Entry;
}

const modules = import.meta.glob<Record<string, unknown>>(["../*.ts", "!../*.test.ts"], { eager: true });

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object";
}

function collect(): Map<string, Slot> {
  const slots = new Map<string, Slot>();
  const walk = (owner: Record<string, unknown>, prefix: string) => {
    for (const [field, value] of Object.entries(owner)) {
      const key = `${prefix}.${field}`;
      if (typeof value === "string" || typeof value === "function") {
        slots.set(key, { owner, field, english: value as Entry });
      } else if (isRecord(value)) {
        walk(value, key);
      }
    }
  };
  for (const [path, module] of Object.entries(modules)) {
    const name = /([^/]+)\.ts$/.exec(path)![1]!;
    for (const [exported, value] of Object.entries(module)) {
      // Only the exported objects are text; a module's own helper
      // functions are not entries.
      if (isRecord(value)) walk(value, `${name}.${exported}`);
    }
  }
  return slots;
}

const slots = collect();

/** Every key, with its English entry. */
export function englishEntries(): ReadonlyMap<string, Entry> {
  return new Map([...slots].map(([key, slot]) => [key, slot.english]));
}

/** Every key, with the entry in use now. */
export function currentEntries(): ReadonlyMap<string, Entry> {
  return new Map([...slots].map(([key, slot]) => [key, slot.owner[slot.field] as Entry]));
}

/** Puts every entry back to its English. */
export function applyEnglish(): void {
  for (const slot of slots.values()) slot.owner[slot.field] = slot.english;
}

/**
 * Uses a translation: each key it names takes the translated string or
 * message; every other key stays English. Problems (`checkMessages`) are
 * skipped, so a half-finished translation still runs.
 */
export function applyMessages(messages: Readonly<Record<string, unknown>>): void {
  applyEnglish();
  const bad = new Set(checkMessages(messages).map((p) => p.key));
  for (const [key, slot] of slots) {
    const message = messages[key];
    if (typeof message !== "string" || bad.has(key)) continue;
    slot.owner[slot.field] =
      typeof slot.english === "string" ? message : (...args: unknown[]) => formatMessage(message, args);
  }
}

/**
 * Changes every entry by `change` (the pseudo-language): strings at once,
 * functions on what they build.
 */
export function applyChanged(change: (text: string) => string): void {
  applyEnglish();
  for (const slot of slots.values()) {
    const english = slot.english;
    slot.owner[slot.field] =
      typeof english === "string"
        ? change(english)
        : (...args: never[]) => {
            const built = english(...args);
            return typeof built === "string" ? change(built) : built;
          };
  }
}

/** Something wrong with one entry of a translation. */
export interface MessageProblem {
  key: string;
  problem: string;
}

/** What is wrong with a translation: unknown keys, bad messages, arguments the entry does not have. */
export function checkMessages(messages: Readonly<Record<string, unknown>>): MessageProblem[] {
  const problems: MessageProblem[] = [];
  for (const [key, message] of Object.entries(messages)) {
    const slot = slots.get(key);
    if (slot === undefined) {
      problems.push({ key, problem: "no such key" });
      continue;
    }
    if (typeof message !== "string") {
      problems.push({ key, problem: "not a string" });
      continue;
    }
    if (message.startsWith("TRANSLATE:")) {
      problems.push({ key, problem: "not translated yet (still the template's English source)" });
      continue;
    }
    let used: Set<number>;
    try {
      used = messageArguments(message);
    } catch (e) {
      problems.push({ key, problem: e instanceof MessageError ? e.message : String(e) });
      continue;
    }
    const allowed = typeof slot.english === "string" ? 0 : slot.english.length;
    const extra = [...used].filter((n) => !Number.isInteger(n) || n >= allowed);
    if (extra.length > 0) {
      problems.push({ key, problem: `uses {${extra.join("}, {")}} but the entry has ${allowed} argument(s)` });
    }
  }
  return problems;
}
