// Which language the page shows, and loading it (docs/09-visual-design.md,
// "Languages"; issue 76).
//
// A translation is a file ui/src/text/locales/<tag>.json: an object of keys
// (./catalogue.ts) to translated strings or messages (./message.ts). Each
// file found there is a language Settings offers. English needs no file;
// it is what the text modules are written in.

import { applyChanged, applyEnglish, applyMessages } from "./catalogue";
import { SOURCE_LANGUAGE, setLanguage } from "./locale";
import { PSEUDO_LANGUAGE, pseudo } from "./pseudo";

type LocaleFile = { default: Record<string, unknown> };

const files = import.meta.glob<LocaleFile>("../locales/*.json");

/** The tag of each translation file: "../locales/pt-BR.json" is "pt-BR". */
const translations = new Map(
  Object.entries(files).map(([path, load]) => [/([^/]+)\.json$/.exec(path)![1]!, load] as const),
);

/**
 * The languages Settings offers: English and every translation. A
 * development build also offers the pseudo-language.
 */
export function availableLanguages(dev: boolean = import.meta.env.DEV): string[] {
  return [SOURCE_LANGUAGE, ...[...translations.keys()].sort(), ...(dev ? [PSEUDO_LANGUAGE] : [])];
}

/**
 * The language to show. A chosen one is used when it is still available.
 * Otherwise the system's languages are tried in order, each first as it is
 * ("pt-BR") and then as its language alone ("pt"); the first that has a
 * translation wins, else English.
 */
export function chooseLanguage(
  chosen: string | null,
  system: readonly string[],
  available: readonly string[] = availableLanguages(),
): string {
  const find = (tag: string) => available.find((a) => a.toLowerCase() === tag.toLowerCase());
  if (chosen !== null) {
    const found = find(chosen);
    if (found !== undefined) return found;
  }
  for (const tag of system) {
    const found = find(tag) ?? find(tag.split("-")[0]!);
    if (found !== undefined) return found;
  }
  return SOURCE_LANGUAGE;
}

/** A language's name in that language: "English", "français", "português (Brasil)". */
export function languageName(tag: string): string {
  if (tag === PSEUDO_LANGUAGE) return pseudo("Pseudo-language");
  try {
    return new Intl.DisplayNames([tag], { type: "language" }).of(tag) ?? tag;
  } catch {
    return tag;
  }
}

/**
 * Shows the page in a language: its text, its formats and the page's
 * `lang`. A translation that will not load leaves English.
 */
export async function showLanguage(tag: string): Promise<void> {
  const root = document.documentElement;
  if (tag === PSEUDO_LANGUAGE) {
    applyChanged(pseudo);
    setLanguage(PSEUDO_LANGUAGE, "en-GB");
    root.lang = "en";
    return;
  }
  const load = translations.get(tag);
  if (tag === SOURCE_LANGUAGE || load === undefined) {
    applyEnglish();
    setLanguage(SOURCE_LANGUAGE);
    root.lang = SOURCE_LANGUAGE;
    return;
  }
  try {
    applyMessages((await load()).default);
    setLanguage(tag);
    root.lang = tag;
  } catch {
    applyEnglish();
    setLanguage(SOURCE_LANGUAGE);
    root.lang = SOURCE_LANGUAGE;
  }
}

/** The system's languages, most wanted first. */
export function systemLanguages(): readonly string[] {
  return typeof navigator === "undefined" ? [] : navigator.languages;
}
