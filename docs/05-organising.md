# Organising

This part covers how a person sorts out an album: star and hide, the photo score, bursts and stacks, sorting, filters, query words, text search, undo, and tidying. It also owns the one definition of `Query`, `QueryResult` and `Row`: what the page sends to Rust and how Rust answers it quickly. Other parts add fields here and refer to it.

Selection, the grid, the viewer and the details panel belong to [Browsing](04-browsing.md). Search by meaning belongs to [Tagging](07-tagging.md). Colours and sizes belong to [Visual design](09-visual-design.md); this document names tokens and never gives colour values.

## Where the work runs

Everything in this document is decided in Rust, in `owlbum-core`. The page holds a `Query`, the selection, and a window of rows. It never filters, sorts, scores or stacks anything itself.

Rust keeps two things for an open album:

| Thing | Where | What it holds |
|---|---|---|
| The album database | `.owlbum/owlbum.sqlite` | The truth: scan facts, typed info, tags, and the `search` table. |
| The album index | Memory, one per open album | A slim copy of what filters and sorts read, plus everything worked out from all its photos together: bursts, quality flags, "interesting", colour and the photo score. |

The album index exists for four reasons. The derived values depend on every photo in the index, so one changed photo can change many others. Writing 100,000 derived rows to a database opened with `synchronous = FULL` is slow, and a view-only album cannot be written at all. Rows in `photos` are wide (they carry the EXIF, analysis and edit JSON), so a filter run in SQL would read the whole table. And the pooled read connections cannot share a temporary table.

Nothing in the album index is information about a photo that could be lost. It is rebuilt from the database on every open. See [The album index](#the-album-index).

## Star and hide

### Keys and controls

| Action | Keys | Also |
|---|---|---|
| Star or unstar | **F** or **S** | The star button on a tile, and Star in the photo menu. |
| Hide or unhide | **H** | Hide in the photo menu. |

- F and S both star in every place: the grid, the timeline, the map, the preview and the full-screen viewer. The old program accepted S only in the viewer.
- The keys do nothing while typing in a field, while a dialog is open, in edit mode, or in a view-only album. Held keys do not repeat.
- The word in the interface is always **Star**, never "Favourite".

### Which photos change

| Situation | Photos changed | New value |
|---|---|---|
| One photo selected | That photo | The opposite of its current value. |
| Two or more selected (the current photo, the one the preview shows, is always among them; see [Browsing](04-browsing.md#selection)) | Every selected photo | The opposite of the current photo's current value, given to all of them. |
| The star button on a tile | That tile's photo only, whatever is selected | The opposite of its current value. |
| In the full-screen viewer | The shown photo only | The opposite of its current value. |
| A closed stack's face is the target | The face only, never the folded members | As above. |

Photos that already have the new value are left alone and are not part of the undo step.

The page sends `photo_set_info(ids, { star })` or `photo_set_info(ids, { hidden })`. Rust writes every row in one transaction, records one undo step, updates the album index, and emits `album_changed`. The change is committed before the command returns, so a second press always sees the first. Presses are sent one after another, and until a row of a later result arrives the page reads a photo's value from what it last sent, so a second press made before the re-run lands still toggles back. A press made while a step is still waiting for its row does nothing.

### Where the selection goes afterwards

A star or a hide can make the photo leave the result (hidden under the default Hidden filter, starred under "Not starred"). The page re-runs its query with `keep` naming the current photo and the selection (see [`query_run`](#query_run)). Rust answers, in `kept`, with the new current photo:

1. The current photo itself, if it is still in the result. The selection is then cut down to the photos still shown.
2. Else, if it was a stack's face and the stack still has shown members, the stack's new face. If it is folded inside a stack that the query opens, the photo itself.
3. Else the tile that now sits in its place: let `k` be the number of tiles that were before it in the previous result and are still present; the tile at index `k` is used.
4. Else (when `k` is past the end) the last tile. If the result is empty, nothing is selected.

In every case the selection becomes the old selection limited to the photos still shown, plus the new current photo (`kept.selection`); so in cases 2 to 4 the new tile joins whatever else of the selection is still shown. The full-screen viewer follows the same rule and closes if the result becomes empty. These are the same rules [Browsing](04-browsing.md#when-the-result-changes) gives for every change to a result; they are stated once in each part and must stay the same.

### The mark

When a star or hide is made with a key, a mark plays over the preview pane, or over the picture in the viewer.

| Change | Mark |
|---|---|
| Starred | A filled star in `--accent`. |
| Unstarred | A star outline in `--mark-off`. |
| Hidden | The eye-off icon in `--mark-off`. |
| Unhidden | An eye outline in `--mark-off`. |

The mark is 180 px (`--size-flag-mark`), centred, and plays for 0.8 s (it grows slightly and fades). A new press restarts it. Its accessible names are "Starred", "Star removed", "Hidden" and "Shown again". With reduced motion it does not move: it is shown for 0.8 s and removed. Clicking the star button on a tile plays no mark; the badge changing is enough.

### Hidden photos

| Rule | Detail |
|---|---|
| Default | Hidden photos are left out of the grid, the timeline, the map, the viewer's stepping, every count and every facet. |
| `Hidden: Show` | They appear in place, dimmed, with the Hidden badge. |
| `Hidden: Only` | Only hidden photos are shown. |
| Score | A hidden photo scores 50 lower. |
| Bursts | A hidden photo is still a member of its burst, but it cannot be the keeper while a member that is not hidden exists. |
| Search | Text search and search by meaning find hidden photos only when they are shown. |
| Files | Hiding never touches the file. Tagging treats hidden photos like any other. |

## Photo score

The photo score is one whole number per photo, with no upper or lower bound. A plain photo lands roughly between 30 and 90. It is computed in Rust over every eligible photo of the open album, never over a filtered list, so a photo's score does not change when a filter does. In a scoped open the eligible photos are those inside the scope.

### Inputs

`analysis` gives `sharpness`, `mean_luma`, `clipped_shadows`, `clipped_highlights`, `saliency_x`, `saliency_y`, `avg_colour` and `colour_grid` ([Media and scanning](03-media-and-scanning.md) defines how they are measured). A photo is **analysed** when `analysis` is not NULL and holds the measures. A video with no poster frame stores `{}` and counts as not analysed.

**Which rows are scored.** The eligible rows are the same ones [Tagging](07-tagging.md#the-two-parts) looks at: inside the open scope, with `missing = 0` and no `scan_error`. Rows with a `scan_error`, rows with `missing = 1` and rows outside the scope are not scored: they are not in the album index, they take no part in the sharpness list or in bursts, and they are in no result. A video is scored from the analysis of its poster frame, the same frame [Media and scanning](03-media-and-scanning.md#image-analysis) makes its medium thumbnail from. Because the album-wide values are worked out over the scope, a photo's score and burst can differ between a scoped open and an open of the whole album.

```text
clamp(x)     = min(1, max(0, x))
MP           = width × height / 1,000,000
S            = the sharpness values of every analysed photo in the album index, sorted ascending; n = its length
percentile(v)= 0.5                                   when n ≤ 1
             = (how many values in S are < v) / (n − 1)   otherwise
variety      = clamp( mean over the 9 grid colours of |colour − mean colour| / √(3 × 255²) )
               (Euclidean distance in RGB; 0 when there is no grid)
d            = hypot(saliency_x − 0.5, saliency_y − 0.5)
D            = hypot(1/6, 1/6)
clipped      = clipped_highlights + clipped_shadows
```

### Lines

Each line is rounded to a whole number (halves away from zero) before the lines are added. A line worth 0 is left out.

| Line | Points | "Why" text |
|---|---|---|
| Focus | `35 × percentile(sharpness)` | "sharper than N % of this album", N = `round(percentile × 100)` |
| Exposure | `20 × clamp(1 − \|mean_luma − 0.48\| × 2.2)` | "on the dark side" when `mean_luma < 0.3`; "on the bright side" when `> 0.68`; else "well exposed" |
| Clipping | `−15 × clamp((clipped − 0.02) / (0.12 − 0.02))`, only when `clipped > 0.02` | "N % blown to white" and "N % crushed to black", joined by ", "; each part only when its share is above 0.005; "clipped highlights or shadows" if neither is |
| Colour variety | `10 × clamp(variety / 0.3)` | "many colours across the frame" when `variety ≥ 0.198`; else "some colour across the frame" |
| Composition | `5 × clamp(1 − \|d − D\| / D)` | "subject near a third, not dead centre" |
| People | `min(10, 5 × number of people on the photo)` | "1 person tagged" or "N people tagged" |
| Size | `10 × clamp(MP / 8)` | "N megapixels": one decimal below 10, whole at 10 and above |
| Burst | `−10` when the photo is in a burst and is not its keeper | "a similar shot in this burst is better" |
| Star | `+50` when starred | "starred" |
| Hidden | `−50` when hidden | "hidden" |
| Your nudges | `score_adjust` | "Up pressed N time(s) more" or "Down pressed N time(s) more" |

Special cases:

| Photo | Lines it gets |
|---|---|
| Not analysed yet | People, Size, Burst, Star, Hidden and Your nudges only. Its tile shows "–" in place of a number. |
| Video | The same lines as a still, from the analysis of its poster frame, but no Size line: a video's frame size says nothing about its quality. A video with no poster frame is not analysed. |

### The score card

The details panel shows a card reading "Photo score" with the total. For a photo that is not analysed it adds "still reading the photo". Hovering or focusing the card opens a pop-up headed "What made this score":

```text
+--------------------------------------------------------------+
| What made this score                                         |
| Focus           sharper than 80 % of this album         +28  |
| Exposure        well exposed                            +19  |
| Colour variety  some colour across the frame             +6  |
| Size            12 megapixels                           +10  |
| Burst           a similar shot in this burst is better  −10  |
| Star            starred                                 +50  |
| Your nudges     Up pressed 2 times more                  +2  |
| ------------------------------------------------------------ |
| Total                                                    105 |
| Up and Down nudge it by 1. Star +50, hidden −50.             |
+--------------------------------------------------------------+
```

- One row per non-zero line, in the order of the table above: the label, the "why" text in `--muted`, and signed points. Positive points use `--score-plus`, negative use `--score-minus` with a real minus sign (U+2212).
- A photo that is not analysed adds the row "Focus, exposure and colour are added once the photo has been read."
- The breakdown comes from `photo_score(id)`. The total is also on every `Row`, so tiles never ask for it.

### Nudges (Up and Down)

| Rule | Detail |
|---|---|
| Keys | **Up** adds 1 to `score_adjust`; **Down** takes 1 away. They work in the grid, timeline, map, preview and viewer. They never move the selection. |
| Targets | The whole selection when it has more than one photo; else the current photo. In the viewer, the shown photo only. |
| Repeat | A held key does not repeat. Each tap is one write and one undo step. |
| Limits | `score_adjust` stays within −100,000 and +100,000. A nudge that would pass the limit leaves that photo unchanged. If no target changed, the toast reads "Can't nudge any further". |
| Command | `photo_nudge(ids, delta)` with `delta` of `+1` or `−1`, the current photo first in `ids`. It replies `{ changed, total, revision }`, where `total` is the current photo's new score, or null while it is not analysed (the one-photo toast then shows the change alone, "+1"). |
| Not available | While typing, in a dialog, in edit mode, in a view-only album. |

**The score toast** sits over the middle of the preview, centred 50% across and 45% down. Its fill is `--score-up-bg` for +1 and `--score-down-bg` for −1, with `--score-toast-text`, and it fades over 1100 ms (no fade with reduced motion; it is simply removed after 1100 ms). Each press restarts it. It reads:

| Case | Text |
|---|---|
| One photo, up | "+1 · 74" (the change, then the new total) |
| One photo, down | "−1 · 72" |
| Several photos | "+1 · 4 photos" |

### Is the score stored?

The score is **not** a column. It lives in the album index as one 32-bit integer per photo, beside the per-photo parts it is built from. Sorting 100,000 photos by score is a sort of an in-memory array and takes a few milliseconds.

It is brought up to date like this:

| What changed | What is recomputed |
|---|---|
| Star, hidden, nudge, or people on a photo | That photo's score. If it is in a burst and the star or hidden flag changed: the burst's keeper, then the score of every member of that burst. |
| A date override on a photo | Bursts for the whole album (dates decide burst membership), then scores. |
| A scan added, removed or re-analysed photos | Everything: the sharpness list, flags, bursts, keepers, "interesting", colour and all scores. |
| Tags arrived | Nothing here. Tags do not feed the score. |

A change of the first kind is applied before the command that made it returns, so the next `query_run` is already correct. The whole-album rebuild is described next.

## The album index

### What it holds per photo

| Group | Fields |
|---|---|
| Identity and order | `id`, folder (an index into the album's folder list), `name_rank` (see [Sorting](#sorting)), kind |
| Filter columns | effective date, star, hidden, `score_adjust`, width, height, has location, number of people, has an edit |
| Analysis | sharpness, mean luma, the two clipping shares, saliency point, dHash as a 64-bit integer, average colour, colour grid |
| Derived | quality flags, `quality`, `interesting`, colour bucket and lightness, burst id, keeper flag, score |

It holds every row inside the open scope with `missing = 0` and no `scan_error`, and nothing for missing rows, rows that could not be read or rows outside the scope, because none of them is ever in a result. In a view-only album it also holds the rows that exist only in memory for the session. At about 150 bytes a photo, 100,000 photos take about 15 MB: a fixed 136-byte record, each file name once in one shared string, each folder once, and the name order at 4 bytes a photo.

### Loading and keeping in step

- **On open** one `SELECT` reads the columns above for every row, on a worker thread. When the schema is current and no journal sits beside the database, it runs on a read-only connection at the same time as the damage check (docs/02, "Backups and damage") rather than after it; the result is kept only if the check passes, and the index is read again after the open otherwise (#124). The grid can show its first window as soon as this finishes. Target: under 2 seconds for 100,000 photos on a local SSD.
- **On every write** the writer thread applies the same change to the index in the same step as the database commit. A write that changes `photos` or `photo_people` goes through `Album::write_photos`: the job reports the photo ids it touched, and after the commit, on the writer thread and before the write answers, the index reads those rows again by id with the same `SELECT` the load uses. A row that is gone, `missing`, has a `scan_error` or is outside the scope leaves the index. A job that fails is rolled back and the index is not touched; if reading the rows again fails after a commit, the write still succeeds and the index is loaded again whole. The index is never read from a half-applied state: readers hold a lock for the length of one query.
- **In a view-only album** there is no writer. The rows the scan keeps only in memory go into the index directly, built by the same rule as a database row, with the same scope test.
- **Derived values** are rebuilt as a new snapshot on a worker thread and swapped in whole. Queries keep using the old snapshot until the swap. The snapshot is keyed by photo id: a photo added after the rebuild started has no entry and reads as unscored, and an entry for a photo that has left is ignored. A changed photo keeps its derived values until the next rebuild, except its score: the index works the score out again as soon as it reads the changed row, against the sharpness list it kept from the last rescore, so a star, hide, nudge or people change is in the next `query_run`. Installing a rebuild's values works every score out again too, because the Burst line reads the new burst and keeper. After a swap Rust emits `album_changed` with reason `facts`.
- **While a scan runs** the whole-album rebuild runs at most once every 2000 ms, and once more when the scan finishes. The scores (the sharpness list, then every photo's score) are worked out again just before each `album_changed` the scan sends (at most once a second, and once at the end), and once on every open, after the index is loaded. Photos found since the last rebuild are shown at once, unanalysed, with "–" for a score.
- **Scope.** Derived values cover the rows in the index. When the album is open scoped to a subfolder, that is the scope only: rows outside it are not scored and are in no burst.

Target for the whole-album rebuild: under 500 ms for 100,000 photos.

## Folder facts

These are album facts: values that can only be worked out by looking at every photo. (The old program called them folder facts.)

### Quality flags

With `S` and `n` as defined under [Inputs](#inputs):

```text
p10    = S[floor(n × 0.1)]
median = S[floor(n / 2)]

blurry  = n ≥ 5  and  sharpness ≤ p10  and  sharpness < median × 0.5
dark    = mean_luma < 0.16  or  clipped_shadows > 0.35
bright  = mean_luma > 0.86  or  clipped_highlights > 0.3
low_res = kind is not video  and  width × height < 1,500,000
```

A photo that is not analysed has no flags. A photo is **low quality** when at least one flag is set.

### Quality

```text
exposure = clamp(1 − |mean_luma − 0.48| × 1.6 − clipped_highlights × 2 − clipped_shadows × 1.5)
quality  = (0.55 × percentile(sharpness) + 0.3 × exposure + 0.15 × clamp(MP / 8)) × (MP < 2 ? 0.6 : 1)
```

This `exposure` is not the Exposure line of the photo score: the factor is 1.6 here and 2.2 there, and clipping is counted here. Both are kept as they are. For a video the `MP` terms use `clamp(MP / 8) = 1` and no 0.6 discount. `quality` is defined only for analysed photos.

### Bursts

A burst is a group of near-identical shots. Two photos **pair** when all of these hold:

| Test | Rule |
|---|---|
| Both have a dHash | A photo without one (not analysed, or a format with no picture) is never in a burst. |
| Time | If both have an effective date, they are no more than 120 minutes apart. If either has no date, there is no time test. |
| Colour | If both have an average colour, the two colours are no more than 70 apart (Euclidean distance in RGB, 0 to 255 per channel). |
| Shape | The Hamming distance of the two 64-bit dHashes is at most **12** when both are dated, and at most **6** when either is not. |

Bursts are the connected groups of pairs (union-find). A group of two or more is a burst. Because pairs chain, a burst can hold two photos that do not pair with each other directly, and a long run of similar shots can span more than 120 minutes.

- **Burst id.** The `id` of the member that sorts first by effective date (undated last), then by `name_rank`.
- **Exact duplicates.** Two files with the same bytes are two rows with a distance of 0, so they form a burst and stack. The old program left exact duplicates out; Owlbum folds them, because that is what a person expects to see.
- **Hidden photos** take part. Photos outside the open scope do not: they are not in the index.

**The keeper** of a burst is the member that comes first in this order:

1. not hidden before hidden;
2. starred before not starred;
3. higher `quality`;
4. earlier effective date (undated last);
5. lower `name_rank`.

A photo in no burst counts as its own keeper. The keeper rule reads `quality`, not the photo score, so the score's Burst line never feeds back into the choice.

**Keeping it fast.** Comparing every pair is 5 billion comparisons at 100,000 photos. Instead:

| Pairs | Method | Cost |
|---|---|---|
| Both dated | Sort by date. Compare each photo with the photos that follow it within 120 minutes, to a limit of the next 2,000. A comparison is one XOR and one bit count. | At most 200 million comparisons; typically far fewer. |
| Either undated | Distance ≤ 6 means that if the 64 bits are cut into 7 segments, at least one segment is identical. Index every hash under each of its 7 segments, and compare only photos that share a segment. | Near linear. |

The 2,000 limit only matters when more than 2,000 photos share one two-hour window. It makes the result depend on nothing but the date order, so it is deterministic.

### Interesting

```text
lo, hi       = the lowest and highest quality among analysed photos in the album
stretched    = hi > lo ? (quality − lo) / (hi − lo) : 0.5
exposure_ok  = (dark or bright) ? 0 : 1
interesting  = 0.45 × stretched + 0.2 × variety + 0.2 × (keeper ? 1 : 0) + 0.15 × exposure_ok
```

`interesting` is 0 for a photo that is not analysed.

### Colour

The colour bucket comes from `avg_colour` (sRGB, 0 to 255 per channel).

```text
r, g, b    = each channel / 255
max, min   = the largest and smallest of r, g, b;  delta = max − min
lightness  = (max + min) / 2
saturation = delta = 0 ? 0 : delta / (1 − |2 × lightness − 1|)
hue        = the usual HSL hue in degrees, 0 ≤ hue < 360
```

| Condition | Bucket |
|---|---|
| saturation < 0.15 | neutral |
| hue < 15 or hue ≥ 345 | red |
| 15 ≤ hue < 40 | orange |
| 40 ≤ hue < 70 | yellow |
| 70 ≤ hue < 165 | green |
| 165 ≤ hue < 200 | cyan |
| 200 ≤ hue < 260 | blue |
| 260 ≤ hue < 290 | purple |
| 290 ≤ hue < 345 | pink |

A photo with no average colour is `neutral` with lightness 0.

## Stacks

A stack folds the shots of a burst into one tile with a count.

| Rule | Detail |
|---|---|
| Turning it on | The **Stack similar** checkbox, titled "Show the near-identical shots of a burst as one photo". It is the `stack_similar` setting, on by default, and remembered. |
| Order of work | Filter first, then sort, then fold. A stack only ever holds photos that pass the filter. |
| Which bursts fold | A burst with two or more members in the filtered list. A burst with one member passing is a plain tile with no badge. |
| The face | The first member in sort order that is starred; else the burst's keeper, if it passes the filter; else the first member in sort order. |
| Position | The stack sits where its face sorts. |
| Badge | "×N", where N is the number of members passing the filter. Its label and tooltip read "Show N similar photos" when closed and "Fold N similar photos" when open. |
| Open | A click on the badge opens or closes the stack. When open, the other members follow the face at once, in sort order, and every member carries the stack outline. |
| Open state | The page keeps the list of open stack ids and sends it in the `Query`. It is forgotten when the album closes. It is not saved in the session. |
| Views | Stacks apply in the grid and the timeline. On the map every photo has its own marker, so the map ignores stacking. |
| Search by meaning | Photos added by meaning are never stacked. |
| Counts | "Showing X of Y" counts photos, not tiles, so folding never changes it. |

**Selection and stacks:**

- Clicking a closed stack selects its face. Star, hide, nudge, delete and drag act on the face only. Select all (Ctrl+A) selects the tiles shown, so folded members are not included.
- When a photo that is folded away must be shown (picked on the map, then a switch to the grid or the viewer; or reached by undo), its stack is opened automatically: the page adds the stack id to its open list and re-runs the query. `query_locate` says which stack a folded photo is in.
- When a stack is closed while one of its other members is selected, the face takes the selection.
- When the face changes (the old face was hidden, or another member was starred), the stack moves to where the new face sorts. If the old face was selected and left the result, the new face is selected.

## Sorting

The Sort control has five options. Each order is total: no two photos ever compare equal, so the same query always gives the same order on Windows and on Linux.

**The name order** is the tie-break in every sort:

1. `file_name`, compared with the natural comparison below;
2. then `rel_dir`, compared the same way;
3. then `id`, ascending.

**The natural comparison** is the string rules of the walker order, steps 2 to 5 of [Media and scanning](03-media-and-scanning.md#order) (`natural_cmp_text`; one function serves both). Each string is split into runs of ASCII digits and runs of everything else. Two digit runs compare as numbers, whatever their length; if the numbers are equal, the run with fewer leading zeros comes first. Two text runs compare by code point after simple lower-casing. A digit run sorts before a text run. A string that ends first comes first. So `IMG_9.jpg` sorts before `img_10.jpg`.

Rust works out the name order once and stores each photo's position as `name_rank`, so no sort compares strings.

| Option | Key | Order |
|---|---|---|
| Date taken (default) | `date` | Effective date, oldest first. Photos with no date come last. Ties by name order. |
| Photo score | `score` | Scored photos first; then score, highest first; ties by name order. A photo not analysed yet ("–") has no score and comes after every scored one. |
| Most interesting | `interesting` | Analysed photos first; then `interesting`, highest first; ties by name order. |
| Main colour | `colour` | Photos with an average colour first; then bucket in the order red, orange, yellow, green, cyan, blue, purple, pink, neutral; then lightness, darkest first; ties by name order. |
| File name | `name` | Name order. |

Rust works the keys of `interesting` and `colour` out from the album index each time it sorts (`quality`, the dark and bright flags, the burst keeper and the average colour), so they always match the index as it is; the range of `quality` is taken over every analysed photo in the index, never over the filtered list. Over 100,000 photos each sort takes under 16 ms on a release build.

The sort is saved in the album's session and restored on reopening. Changing it keeps the selection and scrolls as [Browsing](04-browsing.md#when-the-result-changes) says for any change the user makes: to the current photo, or to the top when there is none. In the timeline view the order is always `date`; the Sort control is disabled there.

## The filter bar

The bar is shown while an album is open and has at least one photo. It is a search landmark labelled "Sort and filter".

```text
+----------------------------------------------------------------------------------------------------------+
| Sort [Date taken   v]  [x] Stack similar  [ Search photos                     ]  [Filters · 2 v]  2,140 photos |
|                                           Try star:yes shape:portrait from:2024-06-01 has:people tag:lake      |
+----------------------------------------------------------------------------------------------------------+
| (Starred ×) (2024-06-01 → 2024-06-30 ×) (lake ×) (Has people ×)        Showing 312 of 2,140 · [Clear filter]  |
+----------------------------------------------------------------------------------------------------------+
```

| Control | Behaviour |
|---|---|
| **Sort** | A select with the five options above. |
| **Stack similar** | A checkbox. |
| **Search box** | Placeholder "Search names, people, places, notes and keywords". Accessible name "Search photos". Width grows from 200 px to 460 px. |
| Hint line | Under the box, announced politely. Normally "Try" followed by the examples `star:yes`, `shape:portrait`, `from:2024-06-01`, `has:people`, `tag:lake`, drawn as the small boxed words [Visual design](09-visual-design.md#typography) defines (no monospace font is bundled). When a query word is bad it shows the error in `--error` instead. |
| **Filters** | A button that opens the panel. It reads "Filters", or "Filters · N" when N panel filters are on, and takes the `--accent` border and text while N > 0. Its chevron turns over while the panel is open. |
| Count | "2,140 photos" or "1 photo", with digit grouping. Shown only while no filter is on. It is the `total` from `query_run`. |

The bar never wraps. In a narrow window it gives up room in the order [Visual design](09-visual-design.md#the-bars) sets: the search box shrinks, then the hint line is hidden and the count shows the number alone. The map is sealed in its own layer, so the bar and the panel always sit above the map's controls.

### The filtered row

The second row is shown only while a filter is on. It is a region labelled "Active filters" on `--panel-2`.

- **Chips.** One chip per filter that is on, in the order of [the list of filters](#the-list-of-filters). Each has a × button named "Remove filter {chip text}" with the tooltip "Remove this filter". Rust supplies the chips; the page draws them and never builds chip text itself.
- **Count.** "Showing X of Y", with digit grouping, announced politely.
- **Clear filter.** A small button. It empties the search box and its error, and clears every filter.

**What X and Y count:**

| Number | Meaning |
|---|---|
| X | Photos that pass: stacks unfolded, and photos matched by meaning included. |
| Y | Every photo in the open scope, with no filter applied. Hidden photos are counted only when the Hidden filter is `Show` or `Only`. |

The old program counted hidden photos in Y and in "N photos" even though the grid left them out. Owlbum does not: with nothing hidden shown, an album of 2,140 photos with 40 hidden reads "2,100 photos". Missing rows are never counted.

### The filter panel

The panel is a popover that drops from the Filters button, its right edge on the button's. It is a labelled group, not a modal dialog (see [Visual design](09-visual-design.md#accessibility-baseline)). It is `min(620px, window width − 24px)` wide and lays its groups out in columns at least 180 px wide.

```text
                                +--------------------------------------------------------------+
                                | PHOTO                WHEN                 WHERE              |
                                | Stars                From                 Folder             |
                                | [All            v]   [ 2024-06-01 ]       [All folders    v] |
                                |   Starred (312)      To                     Top folder (40)  |
                                |   Not starred (1,788)[ 2024-06-30 ]         Day 1 (512)      |
                                | Shape                                       Day 2 (1,548)    |
                                | [Any            v]                        [ ] Has location   |
                                |   Portrait (604)                              (1,930)        |
                                |   Landscape (1,480)                                          |
                                | Hidden                                                       |
                                | [Leave out      v]                                           |
                                |   Show (40)                                                  |
                                |   Only (40)                                                  |
                                |                                                              |
                                | CONTENT              QUALITY                                 |
                                | [ ] Has people (220) [ ] Low quality (96)                    |
                                | (lake ×) (Ann ×)     [ ] Near-duplicates (431)               |
                                +--------------------------------------------------------------+
        (select options are drawn open here only to show the counts; they are ordinary selects)
```

- **Groups**, in order: Photo, When, Where, Content, Quality. Legends are small, upper case, in `--muted`.
- **Folder** is shown only when the open scope has photos in two or more folders.
- **Content** shows the active keyword, object and person chips, each with a ×. With none it shows the note "Pick a keyword under What's in it, or type tag:word in the search box."
- **From** cannot be set later than To, and To cannot be set earlier than From.
- **Opening.** Focus moves to the panel's first control. The panel stays open while several filters are changed.
- **Closing.** Esc closes it, returns focus to the Filters button, and does nothing else. A press outside the panel and its button closes it without moving focus. The button toggles it.
- **Keys.** While focus is inside the panel, the grid's keys (F, S, H, arrows and the rest) do nothing: the key table's typing rule counts the panel like a dialog (see [Desktop, release and tests](08-desktop-and-release.md#the-key-table)).

### Counts beside options (facets)

Each option shows how many photos would be shown if it were chosen. The number in brackets is a **facet**.

| Rule | Detail |
|---|---|
| What is counted | Photos, with stacks unfolded. Photos matched only by meaning are not counted. |
| Which filters apply | Every filter that is on, **except** the one the option belongs to. So "Starred (312)" is the count with every other filter kept and Stars set to Starred. From and To are one filter for this purpose. |
| Hidden | The counts follow the Hidden filter like any other filter. "Show (40)" and "Only (40)" give the number of hidden photos that pass the other filters. |
| Zero | An option with a count of 0 stays enabled and is drawn in `--muted`. |
| Dates | The date fields show no count. |
| When | The page asks for facets when the panel opens (`query_facets(token)`), and passes `want_facets: true` on each `query_run` while the panel stays open. |

Rust works all the facets out in one pass over the album index. For each photo it finds which filters the photo fails. A photo that fails none adds 1 to the matching option of every group. A photo that fails exactly one filter adds 1 to its matching option in that filter's group only. A photo that fails two or more adds nothing.

## The list of filters

One table in `owlbum-core` drives the panel's contents, the chips, the "Filters · N" number, "is anything filtered", the facets and the query words. A Rust test fails if a field of `Filter` has no entry.

| Filter | Group | Panel control | Chip text | Matching rule |
|---|---|---|---|---|
| `star` | Photo | select **Stars**: All / Starred / Not starred | "Starred" / "Not starred" | `yes`: starred. `no`: not starred. |
| `shape` | Photo | select **Shape**: Any / Portrait / Landscape | "Portrait" / "Landscape" | Portrait: `height > width`. Landscape: `width > height`. A square photo matches neither. |
| `hidden` | Photo | select **Hidden**: Leave out / Show / Only | "Hidden shown" / "Only hidden" | Unset: hidden photos fail. `show`: all pass. `only`: only hidden photos pass. |
| `from`, `to` | When | date fields **From** and **To** | "{from} → {to}", "From {from}" or "Until {to}"; one chip, whose × clears both | The day is the first 10 characters of the effective date. It must satisfy `from ≤ day ≤ to`, both ends inclusive. A photo with no date fails. |
| `folder` | Where | select **Folder**: "All folders", "Top folder", then each folder | "Top folder" or the folder's path | The photo's folder equals the value exactly. Photos in folders beneath it do not match. |
| `has_location` | Where | checkbox **Has location** | "Has location" | The photo has an effective place: a typed place, else GPS. |
| `has_people` | Content | checkbox **Has people** | "Has people" | At least one person is on the photo. |
| `people` | Content | chips only | the person's name | Every listed person is on the photo. Names match ignoring case. |
| `tags` | Content | chips only | the keyword or object label | Every listed tag is present: a keyword tag equals one of the photo's keywords, an object tag equals one of its object labels. The lookup is the `photo_labels` table, whose labels are lower case; the typed value is lower-cased and must equal a whole label. |
| `low_quality` | Quality | checkbox **Low quality** | "Low quality" | At least one quality flag is set. |
| `near_duplicates` | Quality | checkbox **Near-duplicates** | "Near-duplicates" | The photo is in a burst. |
| search text | (none) | the search box | no chip | See [Text search](#text-search). |

- A photo passes when it passes every filter that is on.
- **"Is anything filtered"** is true when any filter is on, including search text of the minimum length.
- **"Filters · N"** counts the filters above that are on and have a panel group. From and To count as one. Each person and each tag chip counts as one. Search text is not counted.
- **The folder list** is the distinct folders of the photos in the open scope, with the top folder first and the rest in the natural order. Folder values are paths relative to the album root, so a saved filter still means the same folder when the album is opened at a different scope.
- **The `people` filter and the `person:` query word are new.** The old program could only ask "has anyone". They cost nothing extra, because people now have their own table.
- **Tag matching is fixed.** The old program compared keywords case-sensitively while lower-casing what was typed, so a keyword with a capital letter could never be matched by typing.
- **Lifetime.** The filter and the search text are saved in the album's session and restored when the album is reopened. A saved filter may come from a newer or older Owlbum: unknown fields are ignored, a field with a value that is not allowed is dropped, and a folder that no longer exists is dropped.

## Query words in the search box

Typing `key:value` in the search box turns the word into a filter. The word leaves the box and becomes a chip.

The parser lives in Rust (`query_parse`). The page sends the box text and shows what comes back. It has no table of keys of its own.

### Grammar

```text
box    := word ( space+ word )*      split on spaces that are outside double quotes;
                                     each '"' switches quoting on or off
word   := key ':' value | text
key    := one or more ASCII letters, matched without regard to case against the known keys
value  := '"' chars [ '"' ]  |  chars      a leading '"' is removed, and a trailing '"' if present
```

### Known keys

| Key | Values | Sets | Hint when the value is bad |
|---|---|---|---|
| `star` | `yes`, `no` | `star` | `try star:yes or star:no` |
| `shape` | `portrait`, `landscape` | `shape` | `try shape:portrait or shape:landscape` |
| `hidden` | `show`, `only` | `hidden` | `try hidden:show or hidden:only` |
| `from` | a real calendar day, `YYYY-MM-DD` | `from` | `try from:YYYY-MM-DD` |
| `to` | a real calendar day, `YYYY-MM-DD` | `to` | `try to:YYYY-MM-DD` |
| `folder` | `top`, or a folder's path | `folder` | `no folder named {value}` |
| `has` | `location`, `people` | `has_location` or `has_people` | `try has:location or has:people` |
| `is` | `lowquality`, `duplicate` | `low_quality` or `near_duplicates` | `try is:lowquality or is:duplicate` |
| `tag` | any text that is not blank | adds a keyword tag | `try tag:<word>` |
| `object` | any text that is not blank | adds an object tag | `try object:<word>` |
| `person` | a name that is in the album | adds a person | `no person named {value}` |

Rules for values:

- Fixed values (`yes`, `portrait`, `location` and so on) match without regard to case.
- `from:2024-02-30` is not a real day and is a bad value.
- `folder:` is matched without regard to case against each folder's full path in the open scope, and the filter gets the folder's exact path. `folder:top` means the top folder, and is valid only when the top folder holds a photo. A name that matches no folder is a bad value: the filter is never set to a folder that would show nothing.
- `tag:` and `object:` values are trimmed. The chip shows them as typed; they are compared in lower case against the lower-case labels in `photo_labels`.
- `person:` is matched without regard to case against the album's people, and the filter gets the stored spelling.
- Setting `from` later than an existing `to` (or `to` earlier than `from`) is a bad value with the hint `from must not be after to`.

### Parsing rules

`query_parse(text, final)` splits the box into finished words and one unfinished last word (the text after the last space that is outside quotes).

| Case | Result |
|---|---|
| `final` is false (typing) | The unfinished last word is left alone and stays in the box. |
| `final` is true (Enter) | The last word is parsed too. |
| A word whose key is not known (`http://x`, `a:b`) | Plain text. It stays in the box. No error. |
| A known key with a good value | The word is taken out of the box and its filter is set. |
| The same single-value filter twice (`star:yes star:no`) | The later word wins. No error. |
| The same tag or person twice | Added once. |
| A known key with a bad value (`star:maybe`) | The word stays in the box as text. The first such word sets the error. |
| A space inside an open quote (`folder:"Day `) | The word is not finished, so nothing is taken. The space after the closing quote finishes it. |

The error message is `"{word}" isn't a filter: {hint}`, for example `"star:maybe" isn't a filter: try star:yes or star:no`.

**The search text** that results is the box with the taken words removed, trimmed. While typing, a trailing unfinished word that starts with a known `key:` is also left out of the search text, so typing `lake star:ye` searches for `lake` and not for `star:ye`. On Enter a bad word becomes part of the search text.

### What comes back

```ts
interface ParsedBox {
  taken: boolean;              // true when at least one word became a filter
  set: Partial<Filter>;        // single-value filters to set
  add_tags: TagFilter[];       // tags to add to the filter's list
  add_people: string[];        // people to add, in their stored spelling
  rest: string;                // what the box should now hold
  search: string | null;       // the search text to use; null when empty
  error: string | null;        // the message for the hint line, or null
}
```

### How the box behaves

| Moment | What the page does |
|---|---|
| Any change to the box | Clears the error. If the text ends with a space outside quotes, calls `query_parse(text, false)`. Otherwise it only asks for the search text (the same command; `taken` is false). |
| Enter | Calls `query_parse(text, true)`. |
| A reply with `taken` | Merges `set`, `add_tags` and `add_people` into the filter, replaces the box text with `rest`, shows `error`, and uses `search` as the search text. |
| A reply without `taken` | Shows `error` and uses `search` as the search text. The box text is not touched. |
| The search text changes from elsewhere (Clear filter, a restored session) | The box is replaced with the new text, unless what it holds already gives that search text. |

The query is re-run 200 ms after the last change to the search text, and at once after a filter changes.

### Examples

| Typed | Result |
|---|---|
| `star:yes lake shape:Portrait` then Enter | Chips "Starred" and "Portrait"; the box holds `lake`; search text `lake`. |
| `HAS:People ` | Chip "Has people"; the box is empty. |
| `folder:"day 2" ` | Chip "Day 2" (the folder's own spelling). |
| `tag:Lake object:boat ` | Chips "Lake" and "boat"; photos need the keyword and the object. |
| `person:ann ` | Chip "Ann". |
| `from:2024-06-01 to:2024-06-30 ` | One chip "2024-06-01 → 2024-06-30". |
| `star:maybe lake` then Enter | No chip. The hint line reads `"star:maybe" isn't a filter: try star:yes or star:no`. Search text `star:maybe lake`. |
| `folder:nowhere ` | No chip. `"folder:nowhere" isn't a filter: no folder named nowhere`. |
| `star:yes shape:por` | Chip "Starred"; the box holds `shape:por`; no search text yet. |

## Text search

Text search uses the `search` table (FTS5). It covers eight things about a photo:

| Column | Source |
|---|---|
| `file_name` | The file's name. |
| `description` | The typed description. |
| `notes` | The typed notes. |
| `people` | The names of the people on the photo. |
| `place` | The typed place name. |
| `tag_description` | The description a model wrote. |
| `keywords` | The keyword labels. |
| `objects` | The object labels. |

The old program searched only descriptions, notes, keywords and objects, and looked for the typed text anywhere inside them. Owlbum searches names, people and places as well, and matches whole words and the starts of words.

### Matching

| Rule | Detail |
|---|---|
| Terms | The search text is split on spaces. Text inside double quotes is one phrase. |
| Every term must match | A photo matches when every term is found. Different terms may be found in different columns. |
| Prefix | Each term matches any word that starts with it. `lak` finds "lake" and "Lakeside". A phrase matches its words in order, with the last word as a prefix. |
| No substring match | `ake` does not find "lake". This is a change from the old program. |
| Case and accents | Ignored. `jose` finds "José". This comes from the table's `unicode61 remove_diacritics 2` tokeniser. |
| Punctuation | The tokeniser splits on anything that is not a letter or a digit. `IMG_1234` is the words `img` and `1234`, and is searched as a phrase. |
| Minimum length | The search text needs at least 2 letters or digits in total. Shorter text is not a filter and is ignored. A single-character term inside longer text is kept. |
| No operators | The typed text is never passed to FTS as written. `AND`, `OR`, `NOT`, `*`, `:`, `^` and brackets have no special meaning. |
| Order | Matching does not affect order. Matches are shown in the chosen sort. |

**Building the FTS query.** For each term Rust runs the tokeniser's splitting rule over it, drops the term if nothing is left, wraps what is left in double quotes (doubling any `"`), and adds `*`. Terms are joined with `AND`. So `ann "day 2" IMG_12` becomes `"ann"* AND "day 2"* AND "img 12"*`. Because every term is quoted, a failed query word such as `star:maybe` is searched as the words `star` and `maybe` and can never be a syntax error.

### How it combines

- Text search is one more filter. A photo must match the text **and** pass every other filter. It does not reveal hidden photos.
- It counts as "filtered": the filtered row and Clear filter appear. It has no chip, because the box already shows it.
- **Search by meaning** ([Tagging](07-tagging.md#search-by-meaning)) adds to it. After `query_run` has answered, the page calls `search_meaning(token)`. Rust finds photos close in meaning to the same search text, keeps those that are not already word matches and that pass every other filter (the text filter is left out for this check), and appends them to the end of the list held under that token: the **meaning segment**. They are in score order, best first, and are never stacked. The reply gives `meaning_start`, the index of the first of them, and the new `count` and `photos`; the page never holds a list of ids. The grid starts the segment on a fresh row under the note "Also matching by meaning". The photos count in X, and the viewer and the keys step through them. A new `query_run` makes a new token with no meaning segment until `search_meaning` is called for it.
- **Rows held only in memory** (a view-only album whose database does not know some files) are not in the `search` table. They are matched on file name only, by the same folding and prefix rule.

## The `Query`

This is what the page sends to `query_run`. `Query`, `Keep`, `QueryResult` and `Row` are defined here and nowhere else; [Browsing](04-browsing.md), [Editing and file actions](06-editing-and-file-actions.md) and [Tagging](07-tagging.md) refer to these shapes.

```ts
interface Query {
  view: 'grid' | 'timeline' | 'map';
  timeline_unit: 'auto' | 'day' | 'month';   // used only by the timeline view; see Browsing
  sort: 'date' | 'score' | 'interesting' | 'colour' | 'name';
  stack: boolean;               // Stack similar
  open_stacks: number[];        // ids of stacks shown unfolded
  filter: Filter;
  search: string | null;        // search text, as returned by query_parse
  want_facets: boolean;         // true while the filter panel is open
}

// The optional second argument of query_run: what to follow from the result the page was showing.
interface Keep {
  token: string;                                 // the token of that result
  current: { id: number; index: number } | null; // the current photo
  anchor_id: number | null;                      // where a Shift range starts
  selection: number[];                           // every selected photo id
  view: { id: number; index: number } | null;    // the tile the grid's place is kept by after album_changed (Browsing)
}

interface Filter {
  star?: 'yes' | 'no';
  shape?: 'portrait' | 'landscape';
  hidden?: 'show' | 'only';
  from?: string;                // YYYY-MM-DD
  to?: string;                  // YYYY-MM-DD
  folder?: string;              // relative to the album root; '' is the album root itself
  has_location?: true;
  has_people?: true;
  people?: string[];
  tags?: TagFilter[];
  low_quality?: true;
  near_duplicates?: true;
}

interface TagFilter { kind: 'keyword' | 'object'; value: string }
```

| Field | Rule |
|---|---|
| `view` | `grid`: sort and stack as asked. `timeline`: the sort is forced to `date`; stacking as asked; the answer carries `groups` and `unit`. `map`: every photo that passes the filter is in the result, in the chosen sort, and stacking is ignored; the located ones are read with `query_map` (see [Browsing](04-browsing.md#map-view)). |
| `timeline_unit` | `auto` picks Month when the dated photos of the result span more than 90 days, else Day. Ignored by the other views. |
| `filter` | A filter that is off is absent. `false`, `null` and empty lists are treated as absent. |
| Scope | Not a field. When the album was opened scoped to a subfolder, Rust adds the scope to every query itself. The page cannot widen it. |
| Missing and unreadable rows | Rows with `missing = 1` or a `scan_error` are never returned, whatever the query says. |
| Validation | A query with a value that is not allowed answers `bad_query`. A folder that does not exist, an unknown person and an id in `open_stacks` that is not in the album are not errors: they match nothing or are ignored. |

### `query_run`

`query_run(query, keep?) → QueryResult`.

```ts
interface QueryResult {
  token: string;
  revision: number;             // the album revision this result was built from
  count: number;                // tiles: what query_window pages through
  photos: number;               // X in "Showing X of Y"
  total: number;                // Y, and the "N photos" figure
  album_photos: number;         // photos in the scope that are not missing and could be read, hidden ones
                                // included, at this revision; Browsing uses it to choose the body state
  filtered: boolean;            // is anything filtered
  panel_count: number;          // N in "Filters · N"
  chips: Chip[];
  groups?: Group[];             // timeline view only: one entry per heading, in order
  unit?: 'day' | 'month';       // timeline view only: the unit actually used
  kept: Kept | null;            // where the current photo and the selection landed; null when keep was not passed
  facets?: Facets;              // present when want_facets was true
}

interface Kept {
  current: { id: number; index: number } | null;   // null when the result is empty
  anchor: { id: number; index: number } | null;
  selection: number[];          // the old selection limited to photos still shown, plus the new current photo
}

interface Group {               // timeline view; rules and layout are in Browsing
  key: string;                  // '2026-07-14', '2026-07' or 'none'
  start: number;                // index of the group's first tile
  count: number;                // tiles in the group
  photos: number;               // photos in the group, stacks unfolded, for the heading
}

interface Chip {
  id: string;                   // stable: 'star', 'dates', 'tag:keyword:lake', 'person:Ann', …
  text: string;                 // "Starred", "2024-06-01 → 2024-06-30", "lake"
  remove: Filter;               // the whole filter as it would be with this chip removed
}

interface Facets {
  star: { yes: number; no: number };
  shape: { portrait: number; landscape: number };
  hidden: number;                                  // hidden photos passing the other filters
  folders: { folder: string; label: string; count: number }[];   // label: "Top folder" or the path
  has_location: number;
  has_people: number;
  low_quality: number;
  near_duplicates: number;
}
```

**`keep`** is how the selection survives a change. Every `query_run` after the first passes it: the token of the result the page was showing, the current photo, the anchor and the selection. Rust answers `kept` by the rules under [Where the selection goes afterwards](#where-the-selection-goes-afterwards). If the old token has been dropped, `k` in rule 3 is the old `index` that `keep.current` carried. If the anchor is gone, the anchor becomes the current photo. The page never passes a meaning segment: there is no `meaning_start` in `QueryResult`, because the segment is added afterwards by `search_meaning(token)`, whose reply carries it.

**`Row`** is the one record `query_window` returns for each tile:

```ts
interface Row {
  id: number;
  rel_path: string;             // relative to the album root, '/' separators; for the tooltip and the caption
  file_name: string;
  thumb_key: string;            // photos.thumb_key; builds the owlbum://thumb addresses
  aspect: number;               // width / height, upright; 1 when the dimensions are not known yet
  kind: 'image' | 'raw' | 'video';
  duration_ms: number | null;   // video length; on kind 'image' it marks an animated picture
  star: boolean;
  hidden: boolean;
  score: number | null;         // null draws as "–": not analysed yet
  stack: { id: number; size: number; open: boolean; face: boolean; place: number } | null;
                                // size: members passing the filter. place: this photo's position in the
                                // stack, from 1, for "{place} of {size} in stack". face: this tile is the face.
  edit_plan: EditPlan | null;   // the plan for the stored edit, defined in Editing and file actions;
                                // null when the photo has no edit, or while a save record exists for it
}
```

A photo "has an edit" for the page when `edit_plan` is not null. There is no separate flag.

### How Rust runs a query

| Step | Where it runs | What it does |
|---|---|---|
| 1. Text | SQL, FTS5 | `SELECT rowid FROM search WHERE search MATCH ?` gives the set of matching ids. Skipped when there is no search text. |
| 2. People and tags | SQL, indexed lookups | For each person in `people`: the photo ids from `photo_people`. For each tag: the photo ids from `photo_labels`, by its primary key `(kind, label, photo_id)` (see [Data](02-data.md#schema)). Skipped when neither filter is on. |
| 3. Column filters | Rust, over the album index | One pass over every photo: scope, hidden, star, shape, dates, folder, has location, has people, low quality, near-duplicates, and membership of the id sets from steps 1 and 2. Facets are counted in this same pass. |
| 4. Sort | Rust | Sort the passing ids by the chosen key, using values already in the index. |
| 5. Stack | Rust | Fold bursts, pick faces, and unfold the stacks in `open_stacks`. |
| 6. Keep | Rust | Store the ordered tile list under a new token, with the id sets and the filter needed to check a later meaning segment. Build chips, counts, timeline groups and `kept`. |
| 7. Meaning (later, on `search_meaning(token)`) | Rust | Append the photos found by meaning that pass step 3 without the text filter and are not already present, and record where the segment starts. |

Column filters are not SQL `WHERE` clauses on purpose. The `photos` table has no index that covers them, its rows are wide, and the derived filters (low quality, near-duplicates) and three of the five sorts do not exist in the database at all. A pass over 100,000 slim records in memory takes about a millisecond.

`query_window` reads the display fields of the rows it returns (`rel_path`, `file_name`, `thumb_key`, dimensions, kind, duration and the edit) from the database by primary key, works out `edit_plan` from the stored edit, and takes the score and stack fields from the album index.

**Tokens.** Rust keeps the last 4 results. An older token answers `query_expired`. A result is a list of 32-bit ids and a small stack table: about 0.5 MB for 100,000 photos.

### Performance targets

Measured on the `large-100000` fixture, on a release build, on a local SSD.

| Operation | Target |
|---|---|
| `query_run`, no search text, any combination of column filters, any sort, stacking on | under 30 ms |
| `query_run` with search text that matches up to 10,000 photos | under 100 ms |
| `query_run` with two tags and a person | under 100 ms |
| `query_run` with `want_facets` | no more than 10 ms over the same query without |
| `query_window` of 200 rows | under 10 ms |
| `query_locate` | under 5 ms |
| `query_parse` | under 2 ms |
| Star, hide or nudge on one photo, command start to reply | under 50 ms (one synchronous write is most of it) |
| Whole-album rebuild of the derived values | under 500 ms, off the command threads |
| Loading the album index on open | under 2 s |

The first three rows are the common queries: they are what "stays quick" in the [README](README.md)'s principles means for 100,000 photos. A benchmark for each row lives in `owlbum-core` and is run before a release. It is not a pass-or-fail test, because timings depend on the machine.

**Measured (#26).** `crates/owlbum-core/tests/large_album.rs` opens and scans the `large-100000` fixture, reopens it with its thumbnails kept and times the commands the grid uses (its header says how to run it). Windows development machine, release build, local SSD, one run each, after the two fixes below: `query_run` 1.9 ms by date and 5.3 ms by name; the same re-run keeping the current photo and 1,000 selected, 2.0 ms; `query_window` of 200 rows 0.4 to 1.0 ms; `query_locate` of the last photo 0.1 ms; `query_ids` of the whole result 0.2 ms; one star written through `Album::write_photos`, index included, 8.9 ms (the star command itself is #27). Loading the album index took 0.76 to 1.2 s inside an `Album::open` of 1.0 to 3.0 s, which also runs `quick_check` (0.3 to 0.5 s). After #124 the index is read beside `quick_check`, its name sort takes 86 ms rather than about 300 ms, and `Album::open` takes 0.54 to 0.58 s. Before the fixes, `query_window` took 65 to 110 ms, because each of its 200 lookups was its own read transaction; it now reads the window in one. A re-run with `keep` took 42 ms, almost all of it a hash map of 100,000 places; `settle` now uses a table indexed by id. The rows for filters, search, facets, `query_parse`, the other sorts and the derived rebuild are measured when #28 to #32 build them (#125).

## Undo (Ctrl+Z)

Ctrl+Z takes back the last change to typed info. The undo list is kept by Rust, per open album, in memory.

### What can be undone

| Change | Label | One step is |
|---|---|---|
| Star or unstar | star | One press, for every photo it changed. |
| Hide or unhide | hide | One press, for every photo it changed. |
| Nudge | score | One tap, for every photo it changed. |
| An edit: made, changed or removed | edit, rotate left, rotate right, flip, or revert to original | One "Done" in edit mode (edit), one rotate or flip outside edit mode, or one Revert to original, for every photo it changed. The labels are set by [Editing and file actions](06-editing-and-file-actions.md). |
| People | people | One name added or removed. |
| Description | description | One committed change to the field. |
| Notes | notes | One committed change to the field. |
| Date | date | One committed change. |
| Place | place | One committed change (coordinates and name together). |

A step holds, for each photo, the values of the changed columns as they were before. A command that changes nothing records no step.

**What cannot be undone with Ctrl+Z:**

| Action | Why |
|---|---|
| Delete (to the Recycle Bin or Trash) | The operating system's bin is the way back. |
| Save (an edit written into its file, with `edit_target = file`), Save as… | A file was written. The original that Save replaced is in the Recycle Bin or Trash. |
| Tagging results, Regenerate | They are not typed info. |
| Tidy | Rows are deleted for good, after a confirmation. |
| Sort, filter, view, selection, opening a stack | They change what is shown, not what is known. |

### Rules

| Rule | Detail |
|---|---|
| Depth | 100 steps. The oldest is dropped. |
| Cleared when | The album is closed or another is opened. A step is also dropped when Tidy deletes every photo in it. When an edit is saved into its file, the steps that hold that photo's edits are removed, because the file no longer matches them (see [Editing and file actions](06-editing-and-file-actions.md#save-into-the-file-the-sequence)). |
| Not kept | The list is not saved. It is empty after a restart. |
| Where the key works | The grid, timeline, map, preview and viewer. In a text field Ctrl+Z is the field's own undo. In edit mode it does nothing here; edit mode has its own. |
| Either case | Ctrl+Z with `z` or `Z`, with no Alt or Shift. |
| View-only | Disabled. Nothing can have been changed. |
| Redo | There is none in version 1. |

### What `album_undo` does

1. Takes the newest step. If there is none, it replies `nothing`.
2. Leaves out photos in the step whose rows no longer exist or are missing.
3. Writes the earlier values back in one transaction, without recording a new step, and updates the album index.
4. Emits `album_changed` and replies `{ outcome, label, ids }`.

The page then re-runs its query. If exactly one photo was involved, it is selected, and its stack is opened if it was folded away. Otherwise the selection is settled with `keep`. A photo that was hidden comes back where it sorts.

| Outcome | Toast | Style |
|---|---|---|
| `nothing` | "Nothing to undo" | info |
| `done` | "Undid: {label}" | info |
| `partial` (some photos are gone) | "Couldn't undo all of: {label}" | error |
| `gone` (every photo is gone) | "Couldn't undo {label}: the photo is no longer in the album" | error |

## People suggestions

The People field in the details panel suggests names already used in the album. `album_people()` returns them.

| Rule | Detail |
|---|---|
| Source | The `people` table, with the number of photos each name is on. |
| What is counted | Photos that are not missing. Hidden photos are counted. A name left on no photo is not returned. The old program counted photos whose files had been deleted. |
| Scope | The whole album, even in a scoped open, so a name is spelled the same way everywhere. |
| Order | Most used first; ties in the natural order. |
| In the field | Names already on the photo are left out, ignoring case. Typing narrows the list to names with a word that starts with the typed text, ignoring case and accents. At most 8 are shown. |
| Spelling | Picking a suggestion uses the stored spelling. Typing a name that differs only in case from a stored one also uses the stored spelling: the first spelling wins. Case is compared in full Unicode ("Émile" and "émile" are one person), by Rust before it writes, because SQLite's `NOCASE` folds only ASCII. |
| Clean-up | When the last photo loses a name, the writer deletes that name from `people`. |

The `person:` query word reads the same list.

## Tidy

Tidy removes what the album remembers about photos whose files are gone. It replaces the old program's two-step "orphans" and "forget" calls with one list and one delete, both inside the album database.

A row becomes `missing = 1` when a scan cannot find its file and no new file has the same file key (see [Data](02-data.md)). A missing row keeps its star, notes, people and tags in case the file comes back. Tidy is the only thing that deletes rows.

### The dialog

The top bar has a button **Tidy…** with the tooltip "Remove what Owlbum remembers about photos that are no longer in the album". It opens a wide dialog titled "Tidy album".

```text
+------------------------------------------------------------------------+
| Tidy album                                                         [x] |
|                                                                        |
| These photos are no longer in the album. Removing them deletes their   |
| stars, notes, people and tags for good. No photo file is touched.      |
|                                                                        |
| [x] Day 1                                              3 of 3 selected |
|     [x] IMG_0412.jpg                                                   |
|     [x] IMG_0413.jpg                                                   |
|     [x] IMG_0419.jpg                                                   |
| [-] Day 2                                              1 of 2 selected |
|     [x] IMG_0507.jpg                                                   |
|     [ ] IMG_0511.jpg      starred · notes · 2 people                   |
| [ ] Scans                 This whole folder is gone    0 of 48 selected|
|     ...                                                                |
|                                                                        |
|                                     [Cancel]  [Remove 4 entries]       |
+------------------------------------------------------------------------+
```

| State | What is shown |
|---|---|
| Loading | "Looking…" |
| Nothing missing | "Nothing to tidy: every photo Owlbum knows about is still in the album." and a Close button. |
| A scan is running | "Owlbum is still reading the album. Tidy when it has finished." The list is not shown. |
| Rows found | The explanation above, then the list. The list scrolls at half the window's height. |

- **The list** groups rows by folder, in the natural order. The top folder is shown as "(top folder)". Each folder and each file has a checkbox; a folder's checkbox ticks or clears all its files.
- **What a row would lose** is shown beside it in `--muted`: any of "starred", "hidden", "notes", "description", "N people", "date", "place", "edit", joined by " · ". Tags are not listed, because they can be made again.
- **Ticked by default:** files that would lose nothing typed. **Not ticked by default:** files that have typed info, and every file in a folder that is gone altogether.
- **A folder that is gone altogether** (the folder itself does not exist on disk) is marked "This whole folder is gone", with the tooltip "If it is on a drive that is not plugged in, plug it in and let Owlbum read the album again before tidying."
- **Buttons.** **Cancel**, and the primary **Remove N entries** ("Remove 1 entry"), disabled when N is 0 and while the removal runs.

### Safety rules

| Rule | Detail |
|---|---|
| View-only | The Tidy button is disabled, with a tooltip that says why. |
| During a scan | `album_tidy_list` and `album_tidy_remove` answer `scan_running`. "Missing" is only trustworthy once a scan has finished. |
| The album root cannot be reached | Both commands answer `album_unreachable`, and the dialog says "The album folder can't be reached. Nothing was removed." A whole drive that was unplugged must not look like 100,000 deleted photos. |
| Scope | In a scoped open only rows under the scope are listed and can be removed. |
| Check again before deleting | For each id, Rust looks for the file once more. If it is there now, the row is kept and marked `missing = 0`. |
| Only missing rows | An id whose row is not missing is refused, whatever the page sent. |
| One transaction | All confirmed rows are deleted together. People links, tags and vectors go with them, the rows leave the `search` table, and names left on no photo leave `people`. |
| Photo files | Never touched. Thumbnails in the cache are left to the cache's own pruning. |
| The way back | Tidy cannot be undone with Ctrl+Z. The daily backup in `.owlbum/backup/` still holds the rows until it ages out. |

After a removal the dialog closes and a toast reads "Removed N entries." ("Removed 1 entry."). If some rows were kept because their files had come back, it reads "Removed N entries. M photos were found again and kept." On an error the toast shows the message and the list is loaded again.

## Commands and events

Commands this document adds or defines. All follow the rules in [Architecture](01-architecture.md): `snake_case`, a `Result` with a stable error code, and photos named by id.

| Command | Arguments | Reply | Notes |
|---|---|---|---|
| `query_run` | `query: Query, keep?: Keep` | `QueryResult` | |
| `query_window` | `token, offset, limit` | `Row[]` | `limit` is at most 500. Fewer rows than asked only at the end of the result. |
| `query_locate` | `token, id` | `{ index: number } \| { folded_in: number } \| null` | `folded_in` is the id of the closed stack that holds the photo. `null`: not in the result. |
| `query_facets` | `token` | `Facets` | For the query the token was made from. |
| `query_parse` | `text, final` | `ParsedBox` | Needs an open album, for folder and people names. |
| `photo_set_info` | `ids, patch` | `{ changed: number[], revision: number }` | Defined in [Browsing](04-browsing.md#commands-and-events), which owns the typed fields. This document uses `{ star }` and `{ hidden }`. |
| `photo_nudge` | `ids, delta` | `{ changed: number[], total: number \| null, revision: number }` | `delta` is `1` or `-1`; the current photo goes first and `total` is its new score. |
| `photo_score` | `id` | `{ total, analysed, lines: { label, why, points }[] }` | The score card's breakdown. |
| `album_undo` | none | `{ outcome, label, ids }` | |
| `album_people` | none | `{ name, count }[]` | |
| `album_tidy_list` | none | `{ folder, gone, files: { id, file_name, keeps: string[] }[] }[]` | `gone`: the folder does not exist on disk. `keeps`: what the row would lose. |
| `album_tidy_remove` | `ids` | `{ removed: number, kept: number }` | |

`query_ids` and `query_map` are view commands defined in [Browsing](04-browsing.md#commands-and-events); they read the same token. `search_meaning(token)` is defined in [Tagging](07-tagging.md#commands-and-events).

Error codes. This document uses the shared codes in [Architecture](01-architecture.md#commands): `bad_query`, `query_expired`, `scan_running`, `album_unreachable`, `photo_not_found`, `no_album`, and `view_only` (any write in a view-only album). It adds none.

Events. This document adds no event. It emits `album_changed` with these reasons from the list in Architecture:

| Reason | When |
|---|---|
| `info` | Typed info changed: star, hide, nudge, undo, or a field in the details panel. |
| `facts` | A whole-album rebuild of the derived values was swapped in. |
| `tidy` | Tidy removed rows. |

When the page re-runs its query after `album_changed` is decided by the reason, as the table in Architecture says. It always passes `keep`, so the selection holds. How often it may re-run is set in [Browsing](04-browsing.md#running-a-query).

## Tests

### Rust (`owlbum-core`, on generated fixtures)

| Area | What is tested |
|---|---|
| Score | Each line alone, on a hand-built analysis: the exact points and "why" text, the rounding, and the special cases (not analysed, video, a video with no poster frame). A row with `scan_error` is in no result and has no score. The percentile with 0, 1, 2 and many photos. |
| Score upkeep | After a star, hide, nudge or people change, the stored score equals a from-scratch recomputation, for the photo and for its burst-mates. The same after a simulated scan batch. |
| Flags and quality | Each flag at its threshold, just under and just over. `blurry` needs 5 photos. |
| Bursts | The `trip` fixture's burst is found and nothing else is. Pairs at distance 12 and 13 (dated), 6 and 7 (undated), 120 and 121 minutes, colour distance 70 and 71. The exact duplicate forms a burst. The fast method gives the same groups as comparing every pair, on a seeded album of 5,000 photos. |
| Keeper | Each step of the order, including "a hidden photo is not the keeper while another member is shown". |
| Colour | One colour at each bucket boundary, and the neutral threshold. |
| Sorting | Each option on the `trip` fixture against a written-out expected order. The natural comparison (`IMG_9` before `img_10`, leading zeros, mixed case). No two photos compare equal. |
| Stacks | The face by each rule; a burst with one member passing is a plain tile; open and closed; the face changing after a star and after a hide; the map ignoring stacks. |
| Filters | Every filter alone and in pairs, against expected ids. Square photos match neither shape. Dates are inclusive. A test that fails when a `Filter` field has no entry in the list of filters. |
| Counts | X counts photos, not tiles. Y leaves hidden photos out unless they are shown. Missing rows are never counted. |
| Facets | For each option, the facet equals the `photos` count of the query with that option set. |
| Parser | Every row of the known keys table, every row of the parsing rules table, and every row of the examples table. The error text exactly. `star:constructor` is a bad value, not a crash. |
| Text search | Prefix, phrase, case and accent folding, each of the eight columns, the minimum length, and that `"`, `*`, `:`, `(`, `AND` and `NOT` in the text never cause an error. A photo renamed, re-described or given a person is found by the new text and not the old. |
| Meaning | The segment appended by `search_meaning(token)` comes after word matches, keeps its order, is not stacked, respects the other filters, and the reply's `meaning_start`, `count` and `photos` are right. |
| `keep` | Each of the four selection rules; `kept.selection` holds only photos still shown; a dropped old token falls back to the old index. |
| Row | `query_window` rows carry `rel_path`, `file_name`, `thumb_key`, `edit_plan` (null with no edit and while a save record exists) and the stack's `place`. |
| Undo | Each label. One step per press for many photos. Depth 100. Undo after the photos are missing gives `gone` or `partial`. An undo records no new step. |
| People | Counts, order, missing rows left out, a name removed when unused. |
| Tidy | Lists only missing rows in scope. Refuses a row that is not missing. A file that reappeared is kept and un-marked. Refuses during a scan and when the root is unreachable. Cascades to people links, tags, vectors and the `search` table. |
| View-only | Queries, facets and search work on an album opened read-only, including rows held only in memory. Writes answer `view_only`. |
| Benchmarks | The rows of the performance table, on `large-100000`. Reported, not asserted. |

### Page (Vitest, against the faked API)

| Area | What is tested |
|---|---|
| Keys | F and S star, H hides, Up and Down nudge: in the grid, the preview and the viewer. None of them fire while typing, in the filter panel, or in a view-only album. Held keys send one command. |
| Targets | The ids sent for one photo, for a multi-selection, for a tile's star button, and in the viewer. |
| Selection | After a hide, the page re-runs with `keep` and selects `kept.current`. |
| Mark and toast | The mark for each of the four changes; the toast texts "+1 · 74", "−1 · 72", "+1 · 4 photos", "Can't nudge any further"; no motion with reduced motion. |
| Filter bar | The labels kept for tests: "Sort", "Stack similar", "Search photos", "Filters", "Filters · 2", "2,140 photos", "1 photo", the hint line starting "Try". |
| Filtered row | Chips are drawn from `chips`; × sends the chip's `remove` filter; "Showing 312 of 2,140"; Clear filter empties the box and the filter. |
| Panel | Opens with focus on the first control; Esc closes it, returns focus, and does not reach the grid's keys; a press outside closes it; facet counts are shown; Folder is absent with one folder. |
| Search box | A space after a good word calls `query_parse` and replaces the box with `rest`; a space inside an open quote does not; the error is shown in the hint line and cleared on the next change; Clear filter with a half-typed `star:ye` empties the box. |
| Undo | Ctrl+Z calls `album_undo` and shows the toast for each outcome; it is left to the field while typing. |
| Tidy | Each state; the default ticks; the button's count and its singular form; the toast after removal; disabled in a view-only album. |
| Score card | The rows, the signed points, the footer and the note, from a faked `photo_score`. |

## Open questions

1. **Should a folder filter include the folders beneath it?** It matches exactly, as the old program did. In a deep album (`2024/06/Day 1`), choosing `2024` shows only photos directly in `2024`. Including subfolders may be what people expect, but it changes the meaning of "Top folder".
2. **Newest first.** Date taken sorts oldest first only. A direction toggle is easy for the engine; whether the bar has room for it is a design question.
3. **A filter for kind.** Owlbum shows videos and RAW files, which the old program did not. There is no "Videos only" filter or `is:video` word yet.
4. **Languages without spaces.** The `unicode61` tokeniser treats a run of Chinese, Japanese or Thai text as one word, so only the start of the run can be found. Version 1 is English only; this is recorded so it is not a surprise.
5. **The 2,000 limit on burst comparisons.** It bounds the worst case. It needs checking against a real album of one long event to confirm it never splits a burst a person would notice.
6. **Loading the index over a network share.** The 2-second target is for a local SSD. Reading 100,000 wide rows over a slow share may take much longer; if it does, the slim columns may need their own table or a covering index in [Data](02-data.md).
