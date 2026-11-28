# Translating Owlbum

Owlbum is written in English. Its interface can be shown in another language once someone translates it (issue 76). This page says how a translator adds a language. How the machinery works is in [Visual design, Languages](09-visual-design.md#languages).

## What the owner arranges first

Owlbum has no translations yet, because a translation needs a person who writes the language well. Before any work starts, the owner decides:

1. **Which languages.** A sensible start is the languages of the people who will use Owlbum. Each language is about 700 short entries (the menus, buttons, messages and help text of the page).
2. **Who translates.** A volunteer who speaks the language natively, or a paid translator. Machine translation can make a first draft, but a native speaker must read every entry, since the text is short and context matters ("Star" is a verb on a button and a noun in a filter).
3. **Who reviews.** A second speaker of the language, if one can be found, reads the finished file before it is merged.

Translators do not need to build Owlbum. They need the template file (below) and a way to send the finished file back: a pull request on GitHub, or by email to the owner, who adds it.

## Making the template

A developer makes the starting file, from `ui/`:

```text
npm ci
npm run text:template -- fr.json
```

`fr.json` then holds every key with its English. Most values are plain text. A value that starts with `TRANSLATE:` is an entry that builds its text from values; it shows the English source code, which says what the values are, and the translator replaces it with a message (below). To bring an existing translation up to date after the English changed, pass it as well: `npm run text:template -- fr.new.json src/text/locales/fr.json` keeps every translated entry and adds the new keys with their English.

## Translating

The file is a JSON object of keys to text:

```json
{
  "album.albumText.openFolder": "Ouvrir un dossier",
  "grid.gridText.score": "Note {0}",
  "map.mapText.unlocated": "{0, plural, one {# photo n'a pas de lieu} other {# photos n'ont pas de lieu}}"
}
```

- **Keep the keys** exactly as they are. Translate only the values.
- **A key may be left out.** It then shows in English. So a translation can be merged half-finished and completed later.
- **Plain text** is translated as it is. Keep the meaning, and keep it about as short as the English: buttons and labels have little room. Keep `…` where the English has it (it says a dialog follows).
- **Messages** are for entries that were `TRANSLATE:` in the template. The source code shows the values the entry receives, in order: in `(done, found) => …` the first value, `done`, is `{0}` and the second, `found`, is `{1}`.
  - `{0}` puts in a value. Numbers are written in the language's own way (`2 140`).
  - `{0.fileName}` puts in a field of a value, when the entry receives an object.
  - `{0, plural, one {# photo} other {# photos}}` chooses by count, using the language's own plural forms: `zero`, `one`, `two`, `few`, `many` and `other` (which is required). `=0 {no photos}` matches an exact number. Inside a plural, `#` is the number.
  - `{0, select, true {starred} other {}}` chooses by a value, here a yes-or-no one.
  - Braces `{` and `}` cannot appear as plain text.
- **Leave alone** the entries that only lay out a date (`timeline.timelineText.keyLabel`, `rail.month`, `rail.monthName`): dates, month and day names come from the system in the chosen language.
- **Product names** stay as they are: Owlbum, OpenStreetMap, GitHub, Hugging Face, Windows.

## Adding the file

1. Name the file by the language's tag: `fr.json`, `de.json`, `pt-BR.json` (the language, then the country only when it matters). Put it in `ui/src/text/locales/`.
2. Run `npx vitest run src/text` from `ui/`. It fails, naming the key, on an unknown key, a message that is not well formed, a value the entry does not have (`{2}` in an entry with two values), or a `TRANSLATE:` value left in.
3. Start a development build (`npm --prefix ui run app:dev`), choose the language in Settings > View > Language, restart, and look through the screens for text that is cut off or wraps badly. The pseudo-language in the same list shows every screen with text a third longer, which helps find such places before a translation exists.
4. Add a line to `CHANGELOG.md` under `Unreleased`, Added: "Owlbum can be shown in French."

When the file is merged, Settings lists the language, named in that language ("français"), and a system set to that language uses it by default.

## What is not translated yet

- Text that Rust writes: error messages, the labels of filter chips, the score breakdown, and the native dialogs (the folder picker title, the closing question). This stays English until the Rust side reads the same `language` setting.
- The installer, which offers English only.
- Right-to-left languages (Arabic, Hebrew): the layout does not mirror yet.
