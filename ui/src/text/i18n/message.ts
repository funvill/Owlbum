// The message format of a translation (docs/09-visual-design.md,
// "Languages"): a small part of ICU MessageFormat.
//
//   "Score {0}"                              the first argument
//   "{0.fileName}, {1}"                      a field of an argument
//   "{0, plural, one {# photo} other {# photos}}"
//                                            a plural form; # is the number
//   "{0.star, select, true {starred} other {}}"
//                                            a choice by value
//
// A number is written in the language's own way. A plural may also name an
// exact value: "=0 {no photos}". Braces cannot appear as text.

import { formatNumber, pluralForm } from "./locale";

type Node =
  | { kind: "text"; text: string }
  | { kind: "arg"; path: string }
  | { kind: "pound" }
  | { kind: "plural" | "select"; path: string; options: Map<string, Node[]> };

/** A translation that is not a well-formed message. */
export class MessageError extends Error {}

function parse(source: string): Node[] {
  let at = 0;

  const fail = (why: string): never => {
    throw new MessageError(`${why} at ${at} in "${source}"`);
  };

  const skipSpace = () => {
    while (at < source.length && /\s/.test(source[at]!)) at++;
  };

  const word = (): string => {
    skipSpace();
    const start = at;
    while (at < source.length && /[^\s,{}]/.test(source[at]!)) at++;
    if (at === start) fail("Expected a name");
    return source.slice(start, at);
  };

  const nodes = (inPlural: boolean, nested: boolean): Node[] => {
    const out: Node[] = [];
    let text = "";
    const flush = () => {
      if (text !== "") out.push({ kind: "text", text });
      text = "";
    };
    while (at < source.length) {
      const c = source[at]!;
      if (c === "}") {
        if (!nested) fail("Unexpected }");
        break;
      }
      if (c === "#" && inPlural) {
        flush();
        out.push({ kind: "pound" });
        at++;
        continue;
      }
      if (c !== "{") {
        text += c;
        at++;
        continue;
      }
      flush();
      at++;
      const path = word();
      skipSpace();
      if (source[at] === "}") {
        at++;
        out.push({ kind: "arg", path });
        continue;
      }
      if (source[at] !== ",") fail("Expected , or }");
      at++;
      const type = word();
      if (type !== "plural" && type !== "select") fail(`Unknown type ${type}`);
      skipSpace();
      if (source[at] !== ",") fail("Expected ,");
      at++;
      const options = new Map<string, Node[]>();
      for (;;) {
        skipSpace();
        if (source[at] === "}") {
          at++;
          break;
        }
        const key = word();
        skipSpace();
        if (source[at] !== "{") fail("Expected {");
        at++;
        options.set(key, nodes(type === "plural", true));
        if (source[at] !== "}") fail("Expected }");
        at++;
      }
      if (!options.has("other")) fail("A plural or select needs an other option");
      out.push({ kind: type as "plural" | "select", path, options });
    }
    flush();
    return out;
  };

  const out = nodes(false, false);
  if (at !== source.length) fail("Unexpected }");
  return out;
}

/** The value a path such as "0" or "0.fileName" names among the arguments. */
function lookup(args: readonly unknown[], path: string): unknown {
  const [index, ...fields] = path.split(".");
  let value: unknown = args[Number(index)];
  for (const field of fields) {
    value = value !== null && typeof value === "object" ? (value as Record<string, unknown>)[field] : undefined;
  }
  return value;
}

function show(value: unknown): string {
  if (typeof value === "number") return formatNumber(value);
  if (value === null || value === undefined) return "";
  return String(value);
}

function render(list: Node[], args: readonly unknown[], count: number | null): string {
  let out = "";
  for (const node of list) {
    switch (node.kind) {
      case "text":
        out += node.text;
        break;
      case "pound":
        out += count === null ? "" : formatNumber(count);
        break;
      case "arg":
        out += show(lookup(args, node.path));
        break;
      case "plural": {
        const n = Number(lookup(args, node.path));
        const branch = node.options.get(`=${n}`) ?? node.options.get(pluralForm(n)) ?? node.options.get("other")!;
        out += render(branch, args, n);
        break;
      }
      case "select": {
        const key = String(lookup(args, node.path));
        out += render(node.options.get(key) ?? node.options.get("other")!, args, count);
        break;
      }
    }
  }
  return out;
}

const parsed = new Map<string, Node[]>();

/** The message with the arguments put in. Throws `MessageError` when it is not well formed. */
export function formatMessage(source: string, args: readonly unknown[]): string {
  let list = parsed.get(source);
  if (list === undefined) {
    list = parse(source);
    parsed.set(source, list);
  }
  return render(list, args, null);
}

/** The argument numbers a message uses ({0.fileName} uses 0). Throws `MessageError` when it is not well formed. */
export function messageArguments(source: string): Set<number> {
  const used = new Set<number>();
  const walk = (list: Node[]) => {
    for (const node of list) {
      if (node.kind === "arg" || node.kind === "plural" || node.kind === "select") {
        used.add(Number(node.path.split(".")[0]));
      }
      if (node.kind === "plural" || node.kind === "select") for (const branch of node.options.values()) walk(branch);
    }
  };
  walk(parse(source));
  return used;
}
