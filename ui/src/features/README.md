# Page features

One folder per feature area. A feature folder holds its components, its own
zustand store (`store.ts`), its tests (`*.test.tsx`) and an `index.ts` that
exports what other features may use. Features talk to Rust only through
`api()` from `src/api`, and take their text from `src/text/<area>.ts`.

Build screens from the base controls in `src/controls` and the icons in
`src/icons`, and take every colour, size, duration and z-index from the tokens
in `src/styles/tokens.css` (docs/09-visual-design.md). A page test fails on a
colour, z-index or duration written anywhere else.

Use these names, so parallel issues land in the same places:

| Folder | What | Roadmap |
|---|---|---|
| `app` | Version, start-up, the window shell | 1, 16, 58 |
| `album` | Start screen, recent albums, opening a folder, banners, scan progress | 16, 51 |
| `grid` | The grid, tiles, badges, selection | 18, 19 |
| `preview` | Preview pane, zoom, preload | 20, 21 |
| `viewer` | Full-screen viewer | 22 |
| `details` | Details panel, typed info, histogram | 23, 24, 25 |
| `organise` | Star, hide, score, sorting, stacks, undo, tidy | 27 to 30, 33, 34 |
| `filters` | Filter panel, chips, query words, text search | 31, 32 |
| `timeline` | Timeline view | 35 |
| `map` | Map view | 36 |
| `edit` | Edit mode, light and colour, saving | 43 to 47 |
| `files` | Photo menu, trash, copy, drag | 38 to 42 |
| `tagging` | "What's in it", search by meaning | 55, 56 |
| `settings` | Settings dialog | 57, 59 |
| `help` | Shortcut overlay, About, What's new, failures | 60, 61, 63 |
