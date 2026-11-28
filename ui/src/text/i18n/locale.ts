// The interface language in use, and number, date and plural formats that
// follow it (docs/09-visual-design.md, "Languages"; issue 76). English is
// the source language: the text modules are written in it.

/** The language the text modules are written in. */
export const SOURCE_LANGUAGE = "en";

/**
 * The formats English text uses: day before month ("14 July 2026") and a
 * comma between thousands ("2,140"), as version 1 wrote them.
 */
const SOURCE_FORMATS = "en-GB";

let current = { tag: SOURCE_LANGUAGE, formats: SOURCE_FORMATS };

/** The language in use, as a BCP 47 tag. */
export function language(): string {
  return current.tag;
}

/** Whether the text in use is the English the modules were written in. */
export function isSourceLanguage(): boolean {
  return current.tag === SOURCE_LANGUAGE;
}

/**
 * Sets the language in use. `formats` is the tag Intl formats numbers,
 * dates and plurals with; it defaults to the language itself.
 */
export function setLanguage(tag: string, formats: string = tag === SOURCE_LANGUAGE ? SOURCE_FORMATS : tag): void {
  current = { tag, formats };
}

/** The tag numbers, dates and plurals are formatted with. */
export function formatTag(): string {
  return current.formats;
}

/** A number in the language's own way: "2,140" in English, "2 140" in French. */
export function formatNumber(n: number, options?: Intl.NumberFormatOptions): string {
  return new Intl.NumberFormat(current.formats, options).format(n);
}

/** A date or time in the language's own way. */
export function formatDate(date: Date, options: Intl.DateTimeFormatOptions): string {
  return new Intl.DateTimeFormat(current.formats, options).format(date);
}

/** The plural form a number takes in the language: "one", "few", "other"… */
export function pluralForm(n: number): Intl.LDMLPluralRule {
  return new Intl.PluralRules(current.formats).select(n);
}
