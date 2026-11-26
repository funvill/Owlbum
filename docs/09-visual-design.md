# Visual design

This part says how Owlbum looks: its colours, type, spacing, layers, icons, controls and logo. The other parts say what each screen does; this part says what it is drawn with.

## Starting point

Owlbum keeps the look of the program it replaces: a dark window with an amber accent, and three columns side by side (the thumbnail grid, the preview, the details panel) under a top bar and a filter bar.

Version 1 ships one theme, which is dark. Every colour in the page comes from a named token, so a light theme can be added later by supplying a second set of values. No component may write a colour, a z-index or a duration as a literal. A page test fails if a style outside the token file contains a hex colour, `rgb(` or `hsl(`.

Changes from the old look are few and are listed where they happen. Larger ideas are not built; they are in [Suggestions awaiting a decision](#suggestions-awaiting-a-decision).

## Colour tokens

Tokens are CSS custom properties on `:root`. The theme is chosen by a `data-theme` attribute on the root element, which is `dark` in version 1. The root also sets `color-scheme` to match, so native scrollbars and form parts follow.

The light column is a proposal. It is not built in version 1. It is here to prove the token set is enough to describe a second theme, and its values were checked for contrast in the same way as the dark ones.

### Surfaces and lines

| Token | Dark | Light (proposed) | Use |
|---|---|---|---|
| `--bg` | `#161618` | `#f4f3f0` | Window background, start screen. |
| `--stage` | `#111113` | `#e9e8e4` | Grid column background. |
| `--panel` | `#1d1d20` | `#ffffff` | Bars, details panel, popovers, dialogs, menus. |
| `--panel-2` | `#242428` | `#f3f2ee` | Inputs, default buttons, tile placeholders, notes, the map before tiles load. |
| `--panel-3` | `#2d2d32` | `#e6e4df` | Hover, toasts, chips, the selected segment. |
| `--line` | `#333338` | `#d2d0ca` | Borders of panels, bars, buttons and dialogs. |
| `--line-soft` | `#2a2a2f` | `#e2e0da` | Dividers between sections, splitters. |
| `--line-strong` | `#72727a` | `#86847e` | Borders of inputs, checkboxes and radio buttons. New; see [Contrast](#contrast). |
| `--backdrop` | `rgba(0,0,0,0.55)` | `rgba(0,0,0,0.35)` | The dimmer behind a dialog. |
| `--shadow` | `0 8px 28px rgba(0,0,0,0.5)` | `0 8px 28px rgba(0,0,0,0.18)` | Popovers, menus, dialogs, toasts. |

### Text

| Token | Dark | Light (proposed) | Use |
|---|---|---|---|
| `--text` | `#ecebe7` | `#1d1c1a` | Body text, values, headings. |
| `--muted` | `#9b988f` | `#605d56` | Labels, hints, counts, placeholders. |
| `--disabled` | `#6f6d67` | `#a09d96` | Text and icons of disabled controls only. |

### Accent, state and selection

| Token | Dark | Light (proposed) | Use |
|---|---|---|---|
| `--accent` | `#e7a65f` | `#9a560a` | Primary buttons, the star, active chips, checked boxes, slider fill, progress fill, open-stack outline, map cluster bubbles. |
| `--accent-strong` | `#f2b871` | `#824707` | Primary button hover. |
| `--accent-ink` | `#1b1309` | `#ffffff` | Text and icons on the accent. |
| `--focus` | `#f2b871` | `#824707` | The keyboard focus ring. |
| `--focus-gap` | `#111113` | `#ffffff` | The 2 px gap between a control and its focus ring. |
| `--select` | `#6aa7ff` | `#1b66d6` | Selection outline on tiles, the selected map marker, the info edge. |
| `--ok` | `#6fc28d` | `#1f7a43` | "Saved", success edge and icon. |
| `--warn` | `#e9c45f` | `#855f00` | Warning edge and icon. |
| `--error` | `#ef6e5e` | `#bf3526` | Error text, error edge and icon, clipping flags. |
| `--info-soft` | `#293344` | `#e4edfa` | Background of an information banner. |
| `--ok-soft` | `#2a3731` | `#e4efe8` | Background of a success banner. |
| `--warn-soft` | `#3e382a` | `#f0ece0` | Background of a warning banner. |
| `--error-soft` | `#3f2a2a` | `#f7e7e5` | Background of an error banner. |

### Photo surfaces

These were fixed colours in the old program. They are tokens now. Surfaces that sit on or around a photo stay dark in both themes where a light value would hurt the photo.

| Token | Dark | Light (proposed) | Use |
|---|---|---|---|
| `--preview-bg` | `#111113` | `#e9e8e4` | Preview column background. |
| `--preview-text` | `#ffffff` | `#1d1c1a` | Text drawn in the preview column. |
| `--edit-bar` | `#18181a` | `#f3f2ee` | Edit toolbar background. |
| `--seg-bg` | `#222226` | `#e6e4df` | Segmented control track. |
| `--seg-line` | `#3a3a3e` | `#c4c2bb` | Segmented control border. |
| `--viewer-bg` | `#000000` | `#000000` | Full-screen viewer. Black in both themes. |
| `--viewer-text` | `#ffffff` | `#ffffff` | Viewer file name, counter and close button. |
| `--viewer-muted` | `#b8b5ad` | `#b8b5ad` | Viewer key hints. |
| `--viewer-panel` | `rgba(20,20,22,0.88)` | `rgba(20,20,22,0.88)` | The details box drawn over the viewer. |
| `--badge-bg` | `rgba(0,0,0,0.65)` | `rgba(0,0,0,0.65)` | Pills and buttons drawn on a thumbnail: score, stack count, star, duration. |
| `--badge-text` | `#ffffff` | `#ffffff` | Text and icons on a badge. |
| `--crop-line` | `rgba(255,255,255,0.9)` | `rgba(255,255,255,0.9)` | Crop frame, thirds lines and handles. |
| `--crop-shade` | `rgba(0,0,0,0.55)` | `rgba(0,0,0,0.55)` | The dimmed area outside the crop. |
| `--minimap-line` | `rgba(255,255,255,0.5)` | `rgba(255,255,255,0.5)` | The mini map's border in the preview and the viewer. |

### Edits, score and histogram

| Token | Dark | Light (proposed) | Use |
|---|---|---|---|
| `--marker-edited` | `#c2a37d` | `#7a5a2e` | The edit dot on a tile and the "Edited" badge when `edit_target` is `album`: the edit is kept in the album and nothing is waiting to be saved. A quiet form of the accent. |
| `--marker-unsaved` | `#d98b2b` | `#a35808` | The edit dot on a tile and the "Edited · not saved" badge when `edit_target` is `file`: the edit is not in the file yet. |
| `--marker-ink` | `#1b1309` | `#ffffff` | Text on either edit badge. |
| `--mark-off` | `rgba(255,255,255,0.85)` | `rgba(255,255,255,0.85)` | The star outline, eye and eye-off marks that play over the photo when a star is removed or a photo is hidden or unhidden. The same in both themes, because it sits on a photo. |
| `--drag-placeholder` | `#2d2d32` | `#e6e4df` | The plain square used as the drag icon when a photo has no thumbnail. |
| `--score-up-bg` | `rgba(30,114,58,0.92)` | same | Score toast after a nudge up. |
| `--score-down-bg` | `rgba(201,58,46,0.92)` | same | Score toast after a nudge down. |
| `--score-toast-text` | `#ffffff` | `#ffffff` | Text on either score toast. |
| `--score-plus` | `#4caf6a` | `#1f7a43` | Positive points in the score breakdown. |
| `--score-minus` | `#ef6e5e` | `#bf3526` | Negative points in the score breakdown. |
| `--hist-red` | `#e5483d` | `#d13b30` | Histogram red channel. |
| `--hist-green` | `#3aa655` | `#2e8b47` | Histogram green channel. |
| `--hist-blue` | `#3b82f6` | `#2f6fe0` | Histogram blue channel. |
| `--hist-luma` | `#888888` | `#777777` | Histogram brightness curve. |
| `--hist-fill` | `0.45` | `0.45` | Fill opacity of the three channels. |
| `--hist-luma-fill` | `0.35` | `0.35` | Fill opacity of the brightness curve. |

Three old values were changed so they pass the contrast target:

- The score toast green was `rgba(46,160,82,0.92)`. White text on it measured 3.8:1. It is now `rgba(30,114,58,0.92)`, which measures 6.6:1 over the preview background and 5.0:1 over a white photo.
- The score toast red was `rgba(214,64,52,0.92)`. Over a white photo it measured 4.1:1. It is now `rgba(201,58,46,0.92)`: 5.7:1 over the preview background and 4.5:1 over white.
- Negative score points were `#e05a4f`, 4.2:1 on `--panel-2`. They now use the same value as `--error`, 5.2:1.

### Contrast

The target is WCAG 2.2 level AA: 4.5:1 for text, 3:1 for large text, icons, control borders and focus rings. The ratios below were computed from the token values with the WCAG relative-luminance formula.

| Pair | Dark | Light (proposed) |
|---|---|---|
| `--text` on `--bg` | 15.2 | 15.4 |
| `--text` on `--stage` | 15.8 | 13.9 |
| `--text` on `--panel` | 14.1 | 17.0 |
| `--text` on `--panel-2` | 13.0 | 15.2 |
| `--text` on `--panel-3` | 11.5 | 13.4 |
| `--muted` on `--bg` | 6.3 | 5.9 |
| `--muted` on `--panel` | 5.8 | 6.6 |
| `--muted` on `--panel-2` | 5.4 | 5.9 |
| `--muted` on `--panel-3` | 4.8 | 5.2 |
| `--accent-ink` on `--accent` | 8.8 | 5.7 |
| `--accent-ink` on `--accent-strong` | 10.4 | 7.4 |
| `--accent` on `--panel` | 8.0 | 5.7 |
| `--accent` on `--stage` | 9.0 | 4.6 |
| `--ok` on `--panel` | 7.8 | 5.4 |
| `--warn` on `--panel` | 10.0 | 5.8 |
| `--error` on `--panel` | 5.7 | 5.6 |
| `--error` on `--panel-2` | 5.2 | 5.0 |
| `--select` on `--stage` (outline, needs 3) | 7.7 | 4.4 |
| `--focus` on `--panel` (ring, needs 3) | 9.5 | 7.4 |
| `--line-strong` on `--panel-2` (border, needs 3) | 3.2 | 3.3 |
| `--text` on `--info-soft` | 10.7 | 14.4 |
| `--text` on `--warn-soft` | 9.8 | 14.4 |
| `--text` on `--error-soft` | 11.2 | 14.2 |
| `--marker-ink` on `--marker-unsaved` | 6.7 | 5.3 |
| `--marker-ink` on `--marker-edited` | 7.7 | 6.3 |
| `--badge-text` on `--badge-bg` over a white photo | 7.0 | 7.0 |
| `--viewer-muted` on `--viewer-bg` | 10.3 | 10.3 |

Rules that follow from the numbers:

- **Banner and toast text is `--text`, never `--muted`.** In the dark theme `--muted` on the soft backgrounds measures 4.0 to 4.6, which is not enough.
- **`--muted` is not used on `--panel-3` below 12 px.** At 4.8:1 it passes, with little to spare.
- **`--line` is decoration only.** It measures 1.3:1 on `--panel`, so it cannot be the only thing that marks a control. Inputs, checkboxes and radio buttons use `--line-strong`. Buttons are marked by their fill and label, and keep `--line`.
- **`--disabled` is exempt** from the target, as WCAG allows for disabled controls. A disabled control always has a tooltip that says why.
- **Colour is never the only signal.** Starred, hidden, edited, not-saved, selected and error states each also have an icon, a shape or words.
- **The histogram is a picture of data that is also given as text.** The clipping percentages under it carry the same information.

## Typography

One typeface is bundled: Inter, as a variable font file inside the program. Nothing is fetched. No other font is bundled. The files come from the `@fontsource-variable/inter` package (SIL Open Font License 1.1), which splits the font by script and names it `Inter Variable`; the page loads only the parts a text needs. The font stack is `'Inter Variable', system-ui, sans-serif`, so that file names and notes in scripts Inter does not cover still appear, drawn by a system font.

Each type token below is a `font` shorthand (weight, size, line height and the font stack), used as `font: var(--text-md)`.

| Token | Size / line height | Weight | Use |
|---|---|---|---|
| `--text-xs` | 11 px / 16 px | 600 | Badges on tiles, section labels in capitals. |
| `--text-sm` | 12 px / 16 px | 400 | Small buttons, hints, labels in the details panel, tooltips, query-word examples. |
| `--text-md` | 13 px / 19 px | 400 | Body text. The default for everything. |
| `--text-lg` | 15 px / 20 px | 600 | Dialog titles. |
| `--text-xl` | 20 px / 26 px | 600 | The start screen heading, empty-state headings. |
| `--text-num` | 22 px / 26 px | 700 | The photo score in the details panel. |

- **Weights.** 400 for text, 500 for button labels and values, 600 for headings and file names, 700 for the score number. No other weight is used. Italic is not used. The weights and letter-spacings are tokens too:

| Token | Value | Use |
|---|---|---|
| `--weight-text` | `400` | Text. |
| `--weight-medium` | `500` | Button labels and values. |
| `--weight-heading` | `600` | Headings and file names. |
| `--weight-number` | `700` | The score number. |
| `--tracking-heading` | `-0.01em` | Headings. |
| `--tracking-caps` | `0.06em` | Capital labels. |

- **Headings.** Letter-spacing −0.01em. A section heading in the details panel is `--text-md` at weight 600.
- **Capital labels.** Group labels in the Filters panel are `--text-xs`, capitals, letter-spacing 0.06em, `--muted`.
- **Numbers.** Counts, scores, sizes, coordinates and version numbers use tabular figures.
- **Query-word examples.** Examples such as `star:yes` are set in Inter at `--text-sm`, weight 500, inside a small `--panel-2` box with radius 4 px. No monospace font is used.
- **Text size setting.** There is none in version 1. All sizes are in CSS pixels, so the operating system's display scaling applies.
- **Old small text.** The old 11.5 px size is gone: badges and capital labels use 11 px, everything else that was 11.5 px uses 12 px.
- **Long text.** A file name or path that does not fit is cut with an ellipsis in the middle, so the extension stays readable. The full text is in the tooltip.

## Spacing

The old program used nine gap values with no rule. Owlbum uses eight steps.

| Token | Value | Typical use |
|---|---|---|
| `--space-1` | 2 px | Inside a segmented control; between a badge's icon and text. |
| `--space-2` | 4 px | Between an icon and its label; menu padding; chip gaps. |
| `--space-3` | 8 px | Between controls in a bar; between tiles; bar padding, top and bottom. |
| `--space-4` | 12 px | Bar padding, left and right; details panel padding; grid padding. |
| `--space-5` | 16 px | Dialog padding; between groups of controls. |
| `--space-6` | 24 px | Between sections of a dialog; margin around a photo being edited. |
| `--space-7` | 32 px | Start screen and empty-state blocks. |
| `--space-8` | 48 px | Top of the start screen. |

Old values map onto the scale like this:

| Old value | New step | Note |
|---|---|---|
| 2 px | `--space-1` | Unchanged. |
| 3 px, 4 px | `--space-2` | |
| 6 px | `--space-3`, or `--space-2` between an icon and its label | The gap between tiles grows from 6 to 8 px. |
| 8 px, 10 px | `--space-3` | |
| 12 px | `--space-4` | Unchanged. |
| 14 px, 16 px | `--space-5` | |

A gap that is not on the scale is a defect. One-pixel borders and the sizes in the next section are not gaps and are not on the scale.

## Sizes and radii

| Thing | Size |
|---|---|
| Top bar | 48 px high. |
| Filter bar | 48 px high; 64 px when the query-word hint row shows. |
| Button | 30 px high, 12 px padding left and right. |
| Small button | 26 px high, 8 px padding left and right, `--text-sm`. |
| Icon button | 28 × 28 px, icon 16 px. |
| Text input, select | 30 px high, 8 px padding left and right. |
| Checkbox, radio button | 16 × 16 px. |
| Segmented control | Each segment 26 px high, inside 2 px of padding and a 1 px border: 32 px in all. |
| Slider | Track 4 px high, thumb 14 px. |
| Chip | 22 px high, 8 px padding left and right. |
| Badge on a tile | 18 px high, at least 24 px wide; 20 px high and 28 px wide for the stack count. Placed 4 px from the tile's edge. |
| Menu row | 30 px high; the menu is at least 220 px wide. |
| Progress bar | 6 px high, at most 420 px wide. |
| Splitter | 6 px wide. |
| Thumbnail tile | Square. 96 to 320 px in steps of 8; 160 px by default. |
| Dialog | 440 px wide (small) or 620 px (wide); never wider than the window less 24 px, never taller than the window less 48 px, with its body scrolling. |

A control on the page is at least 24 × 24 px, which is the WCAG 2.2 target-size minimum. The star button on a tile is 24 × 24 px.

The sizes above are tokens as well, so that no component writes a pixel size of its own. Paddings use the spacing steps.

| Token | Value | Use |
|---|---|---|
| `--size-bar` | 48 px | Top bar and filter bar. |
| `--size-bar-tall` | 64 px | Filter bar with the query-word hint row. |
| `--size-control` | 30 px | Buttons, text inputs, selects, menu rows. |
| `--size-control-sm` | 26 px | Small buttons, segments. |
| `--size-icon-button` | 28 px | Icon buttons. |
| `--size-target` | 24 px | The smallest control; the star button on a tile. |
| `--size-icon` | 16 px | Icons. |
| `--size-icon-sm` | 14 px | Icons in small buttons and badges. |
| `--size-icon-lg` | 20 px | Icons in empty states and banners. |
| `--size-check` | 16 px | Checkboxes and radio buttons. |
| `--size-track` | 4 px | Slider track. |
| `--size-thumb` | 14 px | Slider thumb. |
| `--size-chip` | 22 px | Chips. |
| `--size-badge` | 18 px | Badges on tiles. |
| `--size-badge-min` | 24 px | The narrowest badge. |
| `--size-badge-stack` | 20 px | The stack-count badge. |
| `--size-badge-stack-min` | 28 px | The narrowest stack-count badge. |
| `--size-edit-dot` | 8 px | The edit dot on a tile. |
| `--size-menu-min` | 220 px | The narrowest menu. |
| `--size-progress` | 6 px | Progress bar track. |
| `--size-progress-max` | 420 px | The widest progress bar. |
| `--size-splitter` | 6 px | Splitters. |
| `--size-dialog` | 440 px | Small dialog. |
| `--size-dialog-wide` | 620 px | Wide dialog. |
| `--size-tooltip-max` | 280 px | The widest tooltip. |
| `--size-toast-edge` | 3 px | The coloured left edge of a toast. |
| `--size-focus` | 2 px | The focus ring, and the gap inside it. |
| `--size-mark` | 64 px | The owl mark in empty states. |

| Token | Value | Use |
|---|---|---|
| `--radius-sm` | 4 px | Checkboxes, menu rows, example boxes, segments. |
| `--radius-md` | 6 px | Buttons, icon buttons, inputs, the segmented control. |
| `--radius-lg` | 8 px | Tiles, cards, toasts, banners, menus, tooltips. |
| `--radius-xl` | 12 px | Dialogs and popovers. |
| `--radius-full` | 999 px | Chips, badges, radio buttons, slider thumbs, map bubbles. |

The old program used 7 px on buttons; that becomes 6 px. Its single `--radius` token (8 px) is now `--radius-lg`. Its `--font` token is kept and holds the font stack. Its checkboxes were 15 px and are now 16 px.

## Focus

- **The ring.** A control with keyboard focus shows a 2 px ring in `--focus`, 2 px outside the control, with the 2 px between them filled with `--focus-gap`. The gap is what keeps the ring visible on an amber primary button and over a photo.
- **When it shows.** On keyboard focus (`:focus-visible`), never on a mouse click. Text inputs are the exception: they show an `--accent` border on any focus, and the ring as well on keyboard focus.
- **Never removed.** No style sets `outline: none` without drawing this ring in its place.
- **Focus and selection are different things.** Selection is a 2 px `--select` (blue) outline drawn inside the tile. Focus is the amber ring outside it. A tile can show both.
- **Focus is never hidden.** When focus moves to a tile or a row that is scrolled out of view, the view scrolls to show it. A bar that sticks to the top never covers the focused item.
- **Order.** Tab order follows reading order: top bar, filter bar, grid, preview, details panel. The grid is one Tab stop; arrow keys move inside it.

## Motion

| Token | Value | Use |
|---|---|---|
| `--motion-fast` | 100 ms | Hover and pressed colour changes, focus ring. |
| `--motion-base` | 160 ms | Menus, popovers, tooltips and toasts appearing; sections opening. |
| `--motion-slow` | 240 ms | Dialogs and the viewer appearing; the score toast. |
| `--ease` | `cubic-bezier(0.2, 0, 0, 1)` | Everything. |
| `--motion-travel` | 4 px | How far menus, popovers and toasts travel as they appear. |
| `--motion-press` | 1 px | How far a button moves down while pressed. |
| `--progress-unknown-width` | `25%` | The travelling block of a progress bar with no known total. |
| `--progress-unknown-stripes` | `none` | Its stripes: none, so the block is plain. |

Under reduced motion `--progress-unknown-width` becomes `100%` and `--progress-unknown-stripes` a still stripe pattern, which is how the bar stands still without the component testing for the setting.

- **What moves.** Things fade, and menus, popovers and toasts also travel 4 px. Nothing bounces. Nothing in the grid animates when rows load, so scrolling 100,000 photos stays steady.
- **Leaving is quick.** Anything that closes does so in `--motion-fast`.
- **Reduced motion.** When the system asks for reduced motion (`prefers-reduced-motion: reduce`), every duration token becomes 0 ms, nothing travels or scales, the score toast appears and disappears without animation, and a progress bar with no known total shows a still striped bar in place of a moving one. This is done once, by overriding the tokens; components do not test for the setting themselves.

## Cursors

| Where | Cursor |
|---|---|
| Buttons, tabs, menu rows, checkboxes, tiles | The normal arrow. Owlbum is a desktop program; the hand is kept for links. |
| A link that opens the browser or the file manager | `pointer` |
| Text inputs and text that can be selected | `text` |
| Splitters between columns | `col-resize` |
| A zoomed photo that can be panned | `grab`, and `grabbing` while dragging |
| Inside the crop frame | `move` |
| Crop handles | The matching resize cursor (`nwse-resize`, `nesw-resize`, `ns-resize`, `ew-resize`) |
| Disabled controls | The normal arrow. The tooltip says why it is disabled. |
| The whole window while a native dialog is opening | `progress` |

Interface text cannot be selected with the mouse. Values a person may want to copy can be: file names, paths, descriptions, notes, coordinates, camera details and error messages.

## Layering

The old program let the map's internal layers (numbered up to 1100) share one stack with everything else, so its overlays could sit above dialogs and toasts. Owlbum fixes this in two steps.

1. **The map is sealed.** The map container sets `isolation: isolate`. The map library's own layers are then ordered only among themselves, and the map as a whole sits at `--z-base`. The map's own controls (zoom buttons, the "Load maps" card, the offline note) live inside that container.
2. **Everything else uses a named scale.** No component writes a number.

| Token | Value | What sits here |
|---|---|---|
| `--z-base` | 0 | The columns, the grid, the preview, the map (sealed). |
| `--z-raised` | 10 | Badges on tiles, the timeline's sticky heading and month rail, the score toast in the preview, crop handles, splitters while dragged. |
| `--z-bar` | 100 | The top bar, filter bar, banners and progress bars. |
| `--z-popover` | 200 | The Filters panel, the score breakdown, name and country suggestions. |
| `--z-viewer` | 300 | The full-screen viewer. |
| `--z-dialog` | 400 | The dialog backdrop and the dialog. |
| `--z-menu` | 500 | The photo menu and drop-down lists, so they work inside the viewer and inside dialogs. |
| `--z-toast` | 600 | Toasts. |
| `--z-tooltip` | 700 | Tooltips. |

The order, lowest first, is: map and columns, bars, popovers, viewer, dialogs, menus, toasts, tooltips. A page test opens the map, then the viewer, a dialog, a menu and a toast in turn, and checks that each one covers the map.

Popovers, the viewer, dialogs, menus, toasts and tooltips are drawn in one layer root at the end of the page, not inside the component that opened them, so no parent's clipping or stacking can trap them.

## Icons

Owlbum has one icon set, drawn as inline SVG inside the page. There is no icon font and nothing is fetched. The app icon is separate; see [Logo and app icon](#logo-and-app-icon).

The drawings are taken from [Lucide](https://lucide.dev) (ISC License), copied into `ui/src/icons/paths.ts` by `ui/scripts/icons.cjs`, which joins each icon's shapes into one path. The licence is in `notices/lucide.txt`. Each name below is mapped to the Lucide drawing named in the comment beside it in that file; where Lucide has no drawing of the same name the nearest one is used (for example `flip-h` is `move-horizontal`, `straighten` is `ruler`, `tidy` is `brush-cleaning`, `path` is `link`).

Drawing rules:

- A 24 × 24 viewBox, with the drawing inside a 20 × 20 area so there is a 2-unit margin.
- Stroke only: `currentColor`, width 1.7, round caps and round joins, no fill. Two icons have a filled form for the "on" state: `star-filled` and `play`.
- One `<path>` per icon where possible, so the whole set stays small.
- Shown at 16 px by default; 14 px in small buttons and badges; 20 px in empty states and banners.
- Every icon is checked at 16 px on a display at 100% scaling before it is accepted.
- An icon takes the colour of its text. It is never given a colour of its own, except the star, which is `--accent` when on.
- Asking for a name that is not in the set is a build error, not a silent fallback.

| Group | Icons |
|---|---|
| Carried over | `star`, `eye`, `eye-off`, `x`, `check`, `plus`, `folder`, `undo`, `redo`, `rotate-cw`, `crop`, `swap` (swap crop sides), `trash`, `copy`, `image`, `grid`, `download`, `alert`, `info`, `lock`, `sparkle` (tagging) |
| Arrows | `chevron-up`, `chevron-down`, `chevron-left`, `chevron-right`, `arrow-up`, `arrow-down` (score nudge) |
| New: browsing | `timeline`, `map`, `map-pin`, `stack`, `search`, `filter`, `sort`, `calendar`, `person`, `zoom-in`, `zoom-out`, `fit`, `details` (show or hide the details panel) |
| New: video | `video` (a tile is a video), `play`, `pause`, `volume`, `volume-off` |
| New: editing | `rotate-ccw`, `flip-h`, `flip-v`, `straighten`, `sliders` (light and colour), `reset` |
| New: program | `star-filled`, `minus`, `settings` (a gear; the old program used a text character), `keyboard` (shortcut overlay), `rescan`, `tidy`, `close-album`, `warning` (a triangle with an exclamation mark, for banners; `alert` is a circle with one, for errors), `external` (opens outside Owlbum), `reveal` (show in file manager), `path` (copy path), `more` |

`rotate-ccw` is its own drawing, not a mirrored `rotate-cw`, so that it mirrors correctly if a right-to-left language is ever added.

## Window sizes

- **Minimum.** 960 × 600. The window cannot be made smaller.
- **Default.** 1440 × 900 on first start (the `window` setting's default in [Data](02-data.md)), centred, or the screen's work area if that is smaller. After that, the last size, position and maximised state are restored.
- **Scaling.** All sizes here are CSS pixels. Owlbum must look right at 100%, 125%, 150% and 200% display scaling. Borders stay 1 CSS pixel.

### The three columns

| Column | Default | Minimum | Maximum |
|---|---|---|---|
| Grid | 420 px | 200 px | Whatever is left. |
| Preview | The rest | 300 px | None. |
| Details panel | 340 px | 260 px | 560 px |

Two 6 px splitters sit between them. The smallest the three can be together is 772 px, so all three always fit in the minimum window.

When the window narrows, space is taken in this order:

1. The preview shrinks, down to 300 px.
2. Then the details panel shrinks, down to 260 px.
3. Then the grid shrinks, down to 200 px. The grid always shows at least one column of tiles; a tile wider than the column is drawn at the column's width.

Shrinking this way does not change the widths the user chose. When the window grows again the columns return to them. Dragging a splitter does change them, and a drag cannot push any column below its minimum.

All three columns always show. The details panel's close button clears the selection and leaves the column in place, empty, as [Browsing](04-browsing.md#the-details-panel) says; the preview is never narrower than 300 px.

### The bars

Bars never wrap onto a second line and never scroll sideways. As the window narrows they give up room in a fixed order:

| Window width | Top bar | Filter bar |
|---|---|---|
| 1280 px and wider | Everything shows with its label. | Everything shows, with the query-word hint row. |
| 1100 to 1279 px | The album path is cut in the middle. | The search box shrinks towards 200 px. |
| 960 to 1099 px | Buttons other than the primary one become icon buttons with tooltips. The thumbnail-size slider moves into a popover behind the `grid` icon. | The hint row is hidden; the same examples are in the search box's tooltip and in Help. The photo count shows the number alone. |

The Filters panel is `min(620px, window width − 24px)` wide. The "Load maps" card is `min(360px, map width − 24px)` wide.

## Components

Each component is built once and used everywhere. States are listed in the order rest, hover, pressed, focus, disabled, unless noted. "Focus" always means the ring described in [Focus](#focus).

### Buttons

| Kind | Rest | Hover | Pressed | Disabled |
|---|---|---|---|---|
| Primary | `--accent` fill, `--accent-ink` label, weight 600, no border. | `--accent-strong` fill. | `--accent` fill, moved down 1 px. | `--panel-2` fill, `--disabled` label. |
| Default | `--panel-2` fill, 1 px `--line` border, `--text` label, weight 500. | `--panel-3` fill. | `--panel-3` fill, moved down 1 px. | `--panel-2` fill, `--disabled` label. |
| Small | As Default, at the small size. | As Default. | As Default. | As Default. |
| Icon | No fill, no border, `--muted` icon. | `--panel-3` fill, `--text` icon. | `--panel-3` fill. | `--disabled` icon. |
| Icon, on (a toggle that is on) | `--panel-3` fill, `--accent` icon. | `--panel-3` fill. | As rest. | `--disabled` icon. |

- A view has at most one primary button. In a dialog it is the last button on the right.
- A button that deletes or overwrites is a Default button with an `--error` label. It is never primary and never the default action for Enter.
- A button that is busy keeps its width, shows a small spinner in place of its icon, and ignores clicks.
- The 1 px "moved down" press effect is dropped under reduced motion.

### Inputs

| State | Look |
|---|---|
| Rest | `--panel-2` fill, 1 px `--line-strong` border, `--text` text, `--muted` placeholder. |
| Hover | Border `--muted`. |
| Focus | Border `--accent`; the focus ring as well on keyboard focus. |
| Invalid | Border `--error`, with a sentence in `--error` below the field and an `alert` icon. |
| Disabled | Border `--line`, `--disabled` text. |

Selects and date fields look the same and are 30 px high. The search box has a `search` icon inside its left edge and an `x` button inside its right edge once it holds text. A multi-line field (notes) is the same, grows with its text from 3 to 12 lines, then scrolls.

### Checkboxes and radio buttons

| State | Look |
|---|---|
| Off | `--panel-2` fill, 1 px `--line-strong` border. |
| On | `--accent` fill and border; an `--accent-ink` tick (checkbox) or dot (radio). |
| Mixed (checkbox only) | `--accent` fill; an `--accent-ink` dash. |
| Hover | Border `--muted` when off; `--accent-strong` fill when on. |
| Disabled | `--line` border; `--disabled` tick and label. |

The label is part of the click target. A radio group with descriptions (as in Settings) puts the description in `--muted` under the label, and the whole row is the target.

### Slider

A 4 px `--panel-3` track, filled with `--accent` up to the thumb. The thumb is a 14 px `--accent` circle. A slider that has a neutral middle (straighten, light and colour) fills from the middle outwards and returns to the middle on double-click. The current value is shown as a number beside it.

### Segmented control

A row of choices where exactly one is on (Grid, Timeline, Map; crop shapes). The track is `--seg-bg` with a 1 px `--seg-line` border and 2 px padding. A segment at rest has no fill and a `--muted` label. Hover makes the label `--text`. The chosen segment has a `--panel-3` fill and a `--text` label at weight 600. A disabled segment has a `--disabled` label.

### Chips

| Kind | Look |
|---|---|
| Plain (keywords, objects) | `--panel-3` fill, `--text` label, `--text-sm`. Hover: 1 px `--line-strong` border. A plain chip that can be clicked filters by its word. |
| Active filter | `--accent` fill, `--accent-ink` label, with a small `x` that removes it. Hover: `--accent-strong` fill. |
| Count | A plain chip whose label ends with a number, such as "lake ×2". |

### Badges on tiles

Badges sit on photos, so they use `--badge-bg` and `--badge-text` in both themes.

| Badge | Place on the tile | Look |
|---|---|---|
| Score | Bottom right | A pill with the number in `--text-xs`, tabular figures; "–" until the score is known. |
| Stack count | Top right | A pill reading "×3". When the stack is open: `--accent` fill, `--accent-ink` text, and the stack's tiles get a 2 px `--accent` outline. |
| Star | Bottom left | A 24 × 24 px button. Off: shown on hover and on the selected tile only, `star` in `--badge-text`. On: always shown, `star-filled` in `--accent`. |
| Hidden | Top left | An `eye-off` icon; the photo is drawn at 45% opacity. |
| Edit | Top left, after Hidden | An 8 px dot with a 1 px `--badge-bg` ring: `--marker-edited` when `edit_target` is `album`, `--marker-unsaved` when it is `file`. |
| Video | Bottom left, after the star | A pill with a `play` icon and the duration, such as "0:42". |
| Animated picture | Bottom left, after the star | A pill reading "GIF" or "WebP". |
| No picture | Centre | A `warning` icon in `--muted` on a `--panel-2` tile, with the file name under it. Shown when the thumbnail could not be loaded, and with the `video` icon instead for a video that has no poster frame. A file that could not be read at all has no tile: it is in no result. |

The edit badge in the preview and the viewer is a pill with `--marker-ink` text: "Edited" on a `--marker-edited` fill, or "Edited · not saved" on a `--marker-unsaved` fill. Which one shows is decided by [Editing and file actions](06-editing-and-file-actions.md#markers).

### Toasts

Toasts appear at the bottom centre of the window, 16 px above the edge, newest at the bottom, at most three at once. Each is `--panel-3` with `--shadow`, `--radius-lg`, `--text` text, a 3 px coloured left edge and a matching icon.

| Kind | Edge and icon | Stays for |
|---|---|---|
| Info | `--select`, `info` | 4 seconds. |
| Success | `--ok`, `check` | 4 seconds. |
| Error | `--error`, `alert` | Until dismissed. |

A toast may carry one action, such as Undo, as a Small button. A toast with an action stays for 8 seconds. Every toast has an `x`. Hovering or focusing a toast stops its timer. Toasts never take focus.

The score toast is a different thing. It is a large rounded label over the preview that shows the new score after a nudge, in `--score-up-bg` or `--score-down-bg` with `--score-toast-text`, and fades over 1,100 ms, the time [Organising](05-organising.md#nudges-up-and-down) sets.

### Dialogs

A dialog is `--panel`, 1 px `--line` border, `--radius-xl`, `--shadow`, centred over `--backdrop`. It has a title in `--text-lg`, an `x` icon button at the top right, a body, and a row of buttons at the bottom right with the primary one last. Escape and the `x` close it. Clicking the backdrop closes it, unless the dialog holds unsaved input. A dialog that asks a question names its buttons with verbs ("Move to Recycle Bin", "Keep"), never "OK".

### Menus

A menu is `--panel`, 1 px `--line` border, `--radius-lg`, `--shadow`, 4 px padding. A row has a 16 px icon, a label, and its shortcut in `--muted` at the right. Hover or keyboard highlight gives a row a `--panel-3` fill. A disabled row is `--disabled` and has a tooltip that says why. Groups are parted by a 1 px `--line-soft` rule. A row that deletes has an `--error` label. A menu opens where the click was and moves to stay inside the window.

### Banners

A banner is a full-width strip between the bars and the columns. It has a 20 px icon, one sentence in `--text`, and optional Small buttons at the right. More than one can show; they stack in the order below. The other parts say when each appears, what its words and buttons are, and whether it can be dismissed; this table says only what each looks like.

| Banner | Tone | Fill | Icon |
|---|---|---|---|
| View-only, for the reasons `read_only`, `locked`, `chosen` and `newer` | Warning | `--warn-soft` | `lock` in `--warn` |
| Damaged database (view-only for the reason `damaged`) | Error | `--error-soft` | `alert` in `--error` |
| Sync conflict (another copy of the album database was found) | Warning | `--warn-soft` | `warning` in `--warn` |
| Files not shown (the notice that some files are not supported yet or could not be read) | Info | `--info-soft` | `info` in `--select` |
| The first-start note about the update check, while it waits for an answer | Info | `--info-soft` | `info` in `--select` |
| A failure that stops work | Error | `--error-soft` | `alert` in `--error` |

A banner that can be dismissed has an `x` icon button at its right end.

**A scoped album has no banner.** When a subfolder of a larger album is open, the top bar shows the album root and the scope (see [Browsing](04-browsing.md#top-bar)). The scope is drawn in `--text` after the root in `--muted`.

### Progress bars

A progress bar is a 6 px `--panel-3` track with an `--accent` fill and `--radius-full` ends, followed by a label in `--muted` such as "Reading 1,240 of 5,000". When the total is not known, a short `--accent` block travels along the track. Scanning and tagging each have their own strip under the top bar, shown only while they run or need attention; the tagging strip has Pause and Resume as Small buttons. A strip that has stopped on a problem shows its label in `--error` with an `alert` icon.

### Tooltips

`--panel-3` fill, 1 px `--line` border, `--radius-lg`, `--text` at `--text-sm`, at most 280 px wide. A tooltip appears after the pointer rests for 500 ms, or at once on keyboard focus, and goes when the pointer leaves, on Escape, or on any key press. Every icon button has one, and it includes the shortcut where there is one: "Hide (H)".

### Empty states

An empty state is a centred block: the owl mark at 64 px in `--muted`, a heading in `--text-xl`, one sentence in `--muted`, and at most one button.

The words are owned by [Browsing](04-browsing.md#body-states) and are repeated here only to show the pattern.

| Where | Heading | Button |
|---|---|---|
| Start screen, no recent albums | Open a folder of photos | Open folder (primary) |
| An album with no photos | No photos in this folder. | None |
| A filter or search with no results | No photos match the filter | Clear filter |
| The map, when no photo has a place | No photos with a location | None |
| The preview with nothing selected | Select a photo to see it here. | None |
| The details panel with nothing selected | No photo selected | None |

A tile whose thumbnail is not ready is a plain `--panel-2` square of the right shape. It does not shimmer.

## Accessibility baseline

Every feature meets this baseline before it is called done.

**Keyboard.**

- Everything that can be done with the mouse can be done with the keyboard, except dragging a photo out of the window, for which Copy is the keyboard route.
- Tab moves between regions and controls. Arrow keys move inside the grid, menus, segmented controls, radio groups and sliders.
- F6 and Shift+F6 move focus between the top bar, the filter bar, the grid, the preview and the details panel.
- Splitters take focus; Left and Right move them 16 px, and Home and End go to the minimum and maximum.
- Single-letter shortcuts do nothing while a text field has focus.
- Escape closes the topmost thing: tooltip, menu, popover, dialog, then viewer.
- Closing a dialog, menu or the viewer returns focus to the control that opened it.

**Roles and names.**

| Part | Roles and properties |
|---|---|
| Window regions | `banner` for the top bar, `search` for the filter bar, `main` for the grid and preview, `complementary` labelled "Details" for the details panel. |
| Thumbnail grid | `role="grid"` with `aria-multiselectable="true"`, `aria-rowcount` and `aria-colcount` for the whole result, not only the rows drawn. Each row is `role="row"` with `aria-rowindex`; each tile is `role="gridcell"` with `aria-selected`. One cell has `tabindex="0"`; the rest have `-1`. |
| A tile's name | The file name, then its state in words: "IMG_0042.jpg, starred, score 55, stack of 3". |
| Dialogs | `role="dialog"` with `aria-modal="true"` and `aria-labelledby` pointing at the title. Focus is trapped inside. The rest of the page is `inert`. A dialog that only asks to confirm a deletion is `role="alertdialog"`. |
| Viewer | `role="dialog"`, `aria-modal="true"`, labelled with the file name. |
| Menus | `role="menu"`; rows are `menuitem`, or `menuitemcheckbox` where they toggle. The button that opens one has `aria-haspopup="menu"` and `aria-expanded`. |
| Filters panel | A button with `aria-expanded` and `aria-controls`; the panel is a labelled `group`, not a dialog. |
| Segmented control | `role="radiogroup"` with `role="radio"` segments; `role="tablist"` where it switches the grid, timeline and map views. |
| Sections of the details panel | A heading that holds a button with `aria-expanded`. |
| Toasts | Info and success are announced in a `role="status"` region; errors in a `role="alert"` region. |
| Banners | `role="status"`; `role="alert"` for the error banner. |
| Progress bars | `role="progressbar"` with a label, `aria-valuenow` and `aria-valuemax`; no value when the total is unknown. |
| Icon buttons | An `aria-label` with the name the tooltip shows; the tooltip adds the shortcut ("Hide (H)"), which the button also gives in `aria-keyshortcuts`. Toggles add `aria-pressed`. The SVG itself is `aria-hidden`. |
| Photos | The preview and viewer image has the file name as its text alternative, followed by the description when there is one. |

**Seeing.**

- Text and control contrast meet the targets in [Contrast](#contrast).
- The focus ring is always visible, as described in [Focus](#focus).
- Nothing flashes more than three times a second.
- The page works at 200% display scaling in the minimum window without losing a control.
- With Windows contrast themes (`forced-colors: active`), borders and focus rings use system colours and stay visible. Photos and the histogram are left alone.

**Checking.** Page tests run an automated accessibility check (axe) on the grid, the viewer, the Filters panel, Settings and each dialog, and fail on any serious finding. A keyboard-only walk through the main tasks is on the release checklist.

## Logo and app icon

**This is a draft for the owner's approval.** Nothing ships with it until it is approved.

![Draft Owlbum icon: an owl face with two camera-lens eyes on a dark rounded tile](assets/owlbum-icon.svg)

### Concept

An owl, face on, reduced to four shapes. The eyes are the idea: each is a camera lens, with an amber ring for the barrel, dark glass, a six-sided aperture and one highlight. An owl is a bird known for looking carefully, and Owlbum is a program for looking through photos. The name is "owl" plus "album", and the mark shows the first half looking at the second.

| Part | Shape | Colour |
|---|---|---|
| Tile | A square with rounded corners (radius about 20% of the side). | `#1d1d20`, the dark `--panel`. |
| Brow and ear tufts | One thick V-shaped stroke with round ends. | `#ecebe7`, the dark `--text`. |
| Eyes | Two rings that almost touch. | `#e7a65f`, the dark `--accent`. |
| Glass | A disc inside each ring. | `#161618`, the dark `--bg`. |
| Aperture | A hexagon inside each disc. | `#333338`, the dark `--line`. |
| Highlight | A small dot at the upper right of each eye. | `#ecebe7` |
| Beak | A small diamond below and between the eyes. | `#ecebe7` |

The one-colour mark is drawn from this draft in `OwlMark` (`ui/src/controls/`), and an empty state shows it only when asked (`mark`). Until the drawing is approved, no screen asks: empty states show no picture, as suggestion 6 says.

The icon keeps its dark tile in both themes, as app icons do. It uses flat colours only: no gradients, no shadows, no text. The file is hand-written SVG with no outside references, and is the single source for every size.

### How it reads when small

The drawing was looked at in a browser, scaled down to roughly 16, 24, 32, 48 and 64 px. This was a rough check; an exact check of each size happens when the icon files are made. At about 16 px what survives is two amber rings under a pale V on a dark tile, which still reads as an owl; the aperture and highlight are lost, and the beak is a pixel or two.

Because of that, sizes of 24 px and below use a simplified drawing made from the same source, not a scaled-down copy:

- the aperture and highlight are removed;
- the rings are 25% thicker and the brow stroke 30% thicker;
- the beak is 40% larger;
- shapes are nudged onto whole pixels at 16 px.

The simplified drawing is made after the main one is approved.

### Other forms

| Form | Use |
|---|---|
| Tile icon (the drawing above) | The program icon, installers, the website's favicon. |
| Mark without the tile | Inside the page: empty states and the About dialog. Drawn in one colour (`currentColor`), with the eyes as rings. |
| Mark plus the word "Owlbum" | The website header, the About dialog and the README. The word is set in Inter at weight 700, letter-spacing −0.02em, to the right of the mark. |

Clear space around the icon is at least one quarter of its width. The mark is not recoloured, stretched, rotated or placed on a photo.

### Sizes to produce

Tauri's icon command makes the standard set from the 1024 × 1024 source. It cannot swap in the simplified drawing or make every size below, so a second small script makes the 16 and 24 px layers from the simplified drawing and the extra Linux sizes, and rebuilds the `.ico`. The sizes in the "Tauri bundle" row are to be confirmed against what the command produces when the first installer is built.

| Platform | File | Sizes |
|---|---|---|
| Windows | One `.ico` holding several sizes | 16, 24, 32, 48, 64 and 256 px. The 16 and 24 px layers use the simplified drawing. 20 and 40 px layers are added if the taskbar looks soft at 125% scaling. |
| Linux | PNG files in the `hicolor` icon theme, at `hicolor/<size>x<size>/apps/owlbum.png` | 16, 24, 32, 48, 64, 128, 256 and 512 px, plus the SVG at `hicolor/scalable/apps/owlbum.svg`. |
| Tauri bundle | PNG files the bundler expects | 32 × 32, 128 × 128, 256 × 256 (named `128x128@2x`), and 512 × 512. |
| Website | Favicon | The SVG, plus a 32 px PNG and a 180 px PNG. |

## Suggestions awaiting a decision

These are optional. Each has a default that applies if the owner says nothing. The ones marked "in v1" are already written into the sections above and can be reversed; the ones marked "not in v1" are not built.

1. **Follow the system's light or dark setting.** Reason: most desktop programs do, and people who use a light desktop find a lone dark window jarring. Needs the light theme to be built and a three-way setting (System, Dark, Light). Default: not in v1.
2. **Pick the first-run thumbnail size from the window width.** Reason: 160 px tiles look small on a large monitor; 200 px above 1600 px of width shows more of each photo. The user's own choice would still win afterwards. Default: not in v1 (160 px everywhere).
3. **One slimmer combined bar.** Reason: the top bar and filter bar together take about 110 px of height from the photos. One 48 px bar could hold the album name, search, Filters and Sort, with the rest moved into a menu. Default: not in v1 (two bars).
4. **A status bar for progress.** Reason: scan and tagging strips that appear under the top bar push the grid down when they come and go. A 24 px status bar along the bottom would hold both, plus the photo count and selection count, and nothing would jump. Default: not in v1 (strips under the top bar).
5. **A tile hover effect.** Reason: a slight lift (scale 1.02 and a brighter outline) makes the grid feel more alive and shows what a click will pick. It would be off under reduced motion. Default: not in v1 (no hover change beyond showing the star).
6. **The owl in empty states.** Reason: an empty window is the first thing a new user sees, and the mark makes it look intended. Uses the one-colour mark at 64 px; no new artwork. Depends on the logo being approved; without it, empty states show no picture. Default: in v1.
7. **Stronger borders on inputs and checkboxes.** Reason: the old border colour measures 1.3:1 against the panel, well under the 3:1 that level AA asks of a control's edge. `--line-strong` is the one clearly visible change to the old look. Default: in v1.
8. **Darker score toast colours.** Reason: white text on the old green measured 3.8:1. The new green and red pass 4.5:1 even over a white photo, and look a little deeper. Default: in v1.
9. **Query-word examples without a monospace font.** Reason: bundling Inter only means no second typeface; the examples become small boxed words in Inter. Default: in v1.
10. **A high-contrast theme.** Reason: some people need more than level AA. A third token set, chosen when the system asks for more contrast (`prefers-contrast: more`), would raise text to 7:1 and thicken borders and rings. Default: not in v1.
