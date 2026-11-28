// The translation machinery (docs/09-visual-design.md, "Languages"; issue
// 76): messages, plural rules, formats, choosing a language and using a
// translation, and the translation files themselves.

import { afterEach, describe, expect, it } from "vitest";
import { gridText } from "../grid";
import { settingsText } from "../app";
import { timelineText } from "../timeline";
import { applyEnglish, applyMessages, checkMessages, englishEntries } from "./catalogue";
import { chooseLanguage, availableLanguages, languageName } from "./languages";
import { formatNumber, isSourceLanguage, language, pluralForm, setLanguage } from "./locale";
import { MessageError, formatMessage, messageArguments } from "./message";
import { PSEUDO_LANGUAGE } from "./pseudo";

afterEach(() => {
  applyEnglish();
  setLanguage("en");
});

describe("messages", () => {
  it("puts arguments and their fields in", () => {
    expect(formatMessage("Score {0}", [55])).toBe("Score 55");
    expect(formatMessage("{0.fileName} ({1})", [{ fileName: "a.jpg" }, "x"])).toBe("a.jpg (x)");
  });

  it("writes numbers in the language's own way", () => {
    setLanguage("fr");
    // French groups thousands with a narrow no-break space.
    expect(formatMessage("{0} photos", [2140])).toBe(`${formatNumber(2140)} photos`);
    expect(formatNumber(2140)).not.toBe("2,140");
  });

  it("picks the plural form by the language's rules", () => {
    const m = "{0, plural, =0 {none} one {# photo} few {# photos (few)} other {# photos}}";
    expect(formatMessage(m, [0])).toBe("none");
    expect(formatMessage(m, [1])).toBe("1 photo");
    expect(formatMessage(m, [3])).toBe("3 photos");
    setLanguage("pl");
    expect(pluralForm(3)).toBe("few");
    expect(formatMessage(m, [3])).toBe("3 photos (few)");
  });

  it("chooses by value with select", () => {
    const m = "{0.star, select, true {starred} other {plain}}";
    expect(formatMessage(m, [{ star: true }])).toBe("starred");
    expect(formatMessage(m, [{ star: false }])).toBe("plain");
  });

  it("refuses a message that is not well formed", () => {
    expect(() => formatMessage("{0", [])).toThrow(MessageError);
    expect(() => formatMessage("a } b", [])).toThrow(MessageError);
    expect(() => formatMessage("{0, plural, one {x}}", [1])).toThrow(MessageError);
    expect(() => formatMessage("{0, number}", [1])).toThrow(MessageError);
  });

  it("lists the arguments a message uses", () => {
    expect([...messageArguments("{1} and {0, plural, other {# {2.name}}}")].sort()).toEqual([0, 1, 2]);
  });
});

describe("choosing the language", () => {
  const available = ["en", "fr", "pt-BR"];

  it("uses the chosen language when it is available", () => {
    expect(chooseLanguage("fr", ["pt-BR"], available)).toBe("fr");
  });

  it("follows the system's language when a translation of it exists", () => {
    expect(chooseLanguage(null, ["pt-BR", "en"], available)).toBe("pt-BR");
    expect(chooseLanguage(null, ["fr-CA"], available)).toBe("fr");
    expect(chooseLanguage(null, ["de-DE", "fr"], available)).toBe("fr");
  });

  it("falls back to English", () => {
    expect(chooseLanguage(null, ["de-DE"], available)).toBe("en");
    expect(chooseLanguage("ja", [], available)).toBe("en");
  });

  it("offers English always and the pseudo-language only in a development build", () => {
    expect(availableLanguages(false)[0]).toBe("en");
    expect(availableLanguages(false)).not.toContain(PSEUDO_LANGUAGE);
    expect(availableLanguages(true)).toContain(PSEUDO_LANGUAGE);
  });

  it("names a language in that language", () => {
    expect(languageName("en")).toBe("English");
    expect(languageName("fr")).toBe("français");
  });
});

describe("a translation", () => {
  it("replaces the entries it names and leaves the rest English", () => {
    applyMessages({
      "grid.gridText.label": "Photos (fr)",
      "grid.gridText.score": "Note {0}",
      "app.settingsText.languageSystem": "Comme Windows ({0})",
    });
    expect(gridText.label).toBe("Photos (fr)");
    expect(gridText.score(55)).toBe("Note 55");
    expect(settingsText.languageSystem("français")).toBe("Comme Windows (français)");
    expect(gridText.star).toBe("Star");
    applyEnglish();
    expect(gridText.label).toBe("Photos");
    expect(gridText.score(55)).toBe("Score 55");
  });

  it("skips the entries that have problems", () => {
    applyMessages({ "grid.gridText.label": "{0}", "grid.gridText.score": "Note {0" });
    expect(gridText.label).toBe("Photos");
    expect(gridText.score(1)).toBe("Score 1");
  });

  it("is checked for unknown keys, bad messages and extra arguments", () => {
    expect(
      checkMessages({
        "grid.gridText.nope": "x",
        "grid.gridText.label": "{0}",
        "grid.gridText.score": "{0} {1}",
        "grid.gridText.star": 7,
        "grid.gridText.unstar": "{",
        "grid.gridText.starred": "TRANSLATE: (n) => n",
      }).map((p) => p.key),
    ).toEqual([
      "grid.gridText.nope",
      "grid.gridText.label",
      "grid.gridText.score",
      "grid.gridText.star",
      "grid.gridText.unstar",
      "grid.gridText.starred",
    ]);
  });

  it("names months and days in its own language", () => {
    expect(timelineText.keyLabel("2026-07-14")).toBe("Tuesday 14 July 2026");
    setLanguage("fr");
    expect(isSourceLanguage()).toBe(false);
    expect(language()).toBe("fr");
    expect(timelineText.keyLabel("2026-07")).toBe("juillet 2026");
    expect(timelineText.keyLabel("2026-07-14")).toBe("mardi 14 juillet 2026");
  });
});

describe("the translation files", () => {
  const files = import.meta.glob<{ default: Record<string, unknown> }>("../locales/*.json", { eager: true });

  it.each(Object.entries(files))("%s has no problems", (_, file) => {
    expect(checkMessages(file.default)).toEqual([]);
  });

  it("every key is the file name, an exported object and a path", () => {
    for (const key of englishEntries().keys()) expect(key).toMatch(/^[a-z]+\.[A-Za-z]+(\.[A-Za-z0-9_.:-]+)+$/);
  });
});
