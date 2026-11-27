# Editing and file actions

This part covers the edit model, edit mode, how an edited photo is drawn and saved, the photo menu, and the actions that touch files: Recycle Bin or Trash, show in folder, copy, and dragging photos out.

It builds on [Architecture](01-architecture.md) and [Data](02-data.md). Selection, the preview and the viewer belong to [Browsing](04-browsing.md). Undo, star and hide belong to [Organising](05-organising.md). The format registry belongs to [Media and scanning](03-media-and-scanning.md). Colours and sizes are named by token and defined in [Visual design](09-visual-design.md).

## Rules that shape this part

| Rule | What it means here |
|---|---|
| Photo files are only read | Edit mode, rotate and flip change one database field, `photos.edit`. A photo file is written only by **Save** in `file` mode and by **Save as…**, both asked for by the user. |
| Deleting is only the Recycle Bin or Trash | The same holds for the original that a save replaces, and for a file that Save as… replaces. Nothing in this part erases a file. |
| The page names photos by id | Every command here takes photo ids. Rust resolves them to paths, opens the Save dialog, writes the clipboard and starts the drag. The page never passes a path. |
| One implementation of the edit | Every number that turns an edit into pixels is computed in `owlbum-core`. The page draws with numbers Rust gave it. |
| View-only albums | Everything that changes the album or its files is disabled. Each disabled control's tooltip is the first sentence of the view-only banner in force (for example "View only: this folder can't be written to."), as [Data](02-data.md#opening-locking-and-view-only) sets and [Browsing](04-browsing.md#album-banners) words. The table in [View-only](#view-only) lists what still works. |

## The edit model

### `PhotoEdit`

An edit is five things, applied in a fixed order to the **upright** photo (the photo after its EXIF orientation has been applied; `photos.width` and `photos.height` describe it).

| Step | Field | Type and range | Meaning |
|---|---|---|---|
| 1 | `flip` | boolean | Mirror left to right. |
| 2 | `rotate` | 0, 90, 180 or 270 | Turn clockwise by quarter turns, in degrees. |
| 3 | `straighten` | number, −45 to 45 | Turn clockwise by this many more degrees about the centre, then keep the **frame**: the largest centred rectangle with the step 2 aspect ratio that lies wholly inside the turned picture. |
| 4 | `crop` | `{ x, y, w, h }`, fractions 0 to 1 | Cut this rectangle out of the frame. |
| 5 | `colour` | `{ brightness, contrast, saturation, warmth }`, each −100 to 100 | Change light and colour. 0 changes nothing. |

A vertical flip is a horizontal flip plus a 180° turn, so the model has no separate field for it.

```rust
pub struct PhotoEdit {
    pub rotate: u16,          // 0 | 90 | 180 | 270
    pub flip: bool,
    pub straighten: f64,      // degrees clockwise, -45..=45
    pub crop: CropRect,       // fractions of the frame
    pub colour: Colour,
}
pub struct CropRect { pub x: f64, pub y: f64, pub w: f64, pub h: f64 }
pub struct Colour { pub brightness: f64, pub contrast: f64, pub saturation: f64, pub warmth: f64 }

pub const IDENTITY: PhotoEdit = /* rotate 0, flip false, straighten 0, crop {0, 0, 1, 1}, colour all 0 */;
```

### Stored form

`photos.edit` holds the edit as JSON, or NULL for none. Owlbum always writes every field, with numbers rounded to four decimal places:

```json
{"rotate":90,"flip":false,"straighten":-2.5,"crop":{"x":0.1,"y":0.05,"w":0.8,"h":0.9},"colour":{"brightness":10,"contrast":0,"saturation":-20,"warmth":5}}
```

### Parsing, validation and clamping

`parse_edit(json) -> Option<PhotoEdit>` is the only way an edit enters the program, whether it comes from the database or from the page.

| Field | Rule |
|---|---|
| The value | Not a JSON object: invalid. |
| `rotate` | Required. Anything other than 0, 90, 180 or 270: invalid. |
| `flip` | Optional, default false. Present and not a boolean: invalid. |
| `straighten` | Optional, default 0. Present and not a finite number: invalid. Otherwise clamped to [−45, 45]. |
| `crop` | Optional, default the full frame. Present and not an object with finite `x`, `y`, `w`, `h`: invalid. Otherwise `w` and `h` are clamped to [0.01, 1], then `x` to [0, 1 − w] and `y` to [0, 1 − h]. |
| `colour` | Optional, default all 0. Not an object: all 0. Each of the four fields: a finite number is clamped to [−100, 100]; anything else is 0. |
| Other fields | Dropped. |

An invalid edit read from the database is treated as NULL and logged once per photo. An invalid edit sent by the page answers `edit_invalid`.

### Identity and equality

- `is_identity(e)` is true when `rotate` is 0, `flip` is false, `straighten` is 0, all four colour values are 0, and the crop is within 1e-4 of `{0, 0, 1, 1}`.
- `same_edit(a, b)` is true when `rotate` and `flip` are equal and every number is within 1e-4. No edit counts as identity.
- An identity edit is never stored. Writing one stores NULL (see [Data](02-data.md)).
- `edit_key(e)` is the 32-bit FNV-1a hash, in base 36, of the text `rotate,flip,straighten,x,y,w,h,brightness,contrast,saturation,warmth`, with `flip` as 0 or 1 and each other number written as `round(v × 10000)`. Equal edits share a key. The page uses it to share colour filters and to key drawn images.

### Geometry

```text
turned_size(w, h, rotate) = (h, w) when rotate is 90 or 270, else (w, h)

straighten_scale(w, h, deg):
    a = |deg| in radians
    k = min( w / (w·cos a + h·sin a),  h / (w·sin a + h·cos a) )
    return min(1, k)
```

`straighten_scale` is how much the turned picture's size shrinks to give the frame. It is 1 when `deg` is 0.

`edit_plan(w, h, e)` gives the steps in whole pixels for an upright photo of `w × h`:

```text
t      = turned_size(w, h, e.rotate)
k      = straighten_scale(t.w, t.h, e.straighten)
fw     = max(1, round(t.w × k))
fh     = max(1, round(t.h × k))
left   = clamp(round(e.crop.x × fw), 0, fw − 1)
top    = clamp(round(e.crop.y × fh), 0, fh − 1)
width  = clamp(round(e.crop.w × fw), 1, fw − left)
height = clamp(round(e.crop.h × fh), 1, fh − top)

plan = {
    key:    edit_key(e),
    pic:    { w, h },
    flip:   e.flip,
    angle:  e.rotate + e.straighten,          // degrees clockwise
    frame:  { w: fw, h: fh },
    crop:   { left, top, width, height },     // inside the frame
    out:    { w: width, h: height },
    colour: None when all four values are 0, else colour_matrix(e.colour)
}
```

The frame is centred on the turned picture's centre. `round` rounds half away from zero.

### Turning and flipping an edit

These keep the same part of the picture cropped.

```text
rotate_left(e):   rotate = (rotate + 270) mod 360
                  crop   = { x: y,          y: 1 − x − w,  w: h,  h: w }

rotate_right(e):  rotate = (rotate + 90) mod 360
                  crop   = { x: 1 − y − h,  y: x,          w: h,  h: w }

flip(e):          flip       = not flip
                  rotate     = (360 − rotate) mod 360      // 90 and 270 swap
                  straighten = −straighten
                  crop       = { x: 1 − x − w,  y,  w,  h }
```

`flip` mirrors what is on screen. Because the stored `flip` step comes before the turn, mirroring the result also reverses the turn and the straighten angle. `straighten` and `colour` are unchanged by a quarter turn. `rotate_left` four times, `rotate_right` four times and `flip` twice each give back the starting edit.

### Aspect presets

| Preset | Ratio (width ÷ height) | Swap allowed |
|---|---|---|
| Free | none | no |
| Original | the turned photo's own shape | yes, unless the photo is square |
| Square | 1 | no |
| 4:5 | 0.8 | yes |
| 3:2 | 1.5 | yes |
| 16:9 | 16 ÷ 9 | yes |

Swapping uses the inverse ratio. Choosing a preset refits the crop with `fit_crop_to_aspect`: the largest rectangle of that ratio, centred on the old crop's centre and moved to stay inside the frame.

```text
fit_crop_to_aspect(crop, ratio, frame):
    rf = ratio × frame.h / frame.w            // the ratio in fractions of the frame
    h  = min(1, 1 / rf)
    w  = h × rf
    cx = crop.x + crop.w / 2;  cy = crop.y + crop.h / 2
    return { x: clamp(cx − w/2, 0, 1 − w),  y: clamp(cy − h/2, 0, 1 − h),  w,  h }
```

### Minimum crop sizes

| Limit | Value | Where |
|---|---|---|
| Smallest crop `parse_edit` accepts | 0.01 of the frame, each way | Rust |
| Smallest crop the handles can make | 0.05 of the frame, each way | Page |
| Smallest width with a fixed ratio | `max(0.05, 0.05 × rf)` | Page |
| Smallest output | 1 pixel each way | `edit_plan` |
| Tolerance for "the same" | 1e-4 | Rust |

### Colour

The four sliders become **one 3×3 matrix plus an offset**, computed by one pure function. It works on sRGB-encoded values from 0 to 1. The order is brightness, contrast, saturation, warmth.

```text
colour_matrix(c):
    b = (c.brightness / 100) × 0.4            // brightness +100 adds 0.4 to every channel
    k = max(0, 1 + c.contrast / 100)          // contrast scales about 0.5
    s = 1 + c.saturation / 100                // 0 is grey, 2 is double
    w = c.warmth / 100
    warm = [ 1 + 0.15·w,  1,  1 − 0.15·w ]    // red up and blue down when warm
    L    = [ 0.2126, 0.7152, 0.0722 ]         // Rec. 709 luma

    for row r in 0..3, column j in 0..3:
        m[r][j] = warm[r] × k × ( (1 − s) × L[j] + (s when r = j, else 0) )

    o = k × b + 0.5 × (1 − k)
    offset[r] = warm[r] × o

    values closer to 0 than 1e-12 are written as 0
    return { m (9 numbers, row-major), offset (3 numbers) }

apply:  out = clamp(m · in + offset, 0, 1)        per pixel, alpha untouched
8-bit:  out8 = clamp(round(out × 255), 0, 255)
```

This is the same as doing the four steps one after another: `v + b`, then `k × (v − 0.5) + 0.5`, then the saturation mix (whose rows add up to 1, so a grey stays grey), then the per-channel warmth scale.

| Check | Result |
|---|---|
| All four 0 | The identity matrix and a zero offset. |
| Brightness +50 | Offset `[0.2, 0.2, 0.2]`, identity matrix. |
| Contrast −100 | A zero matrix and offset `[0.5, 0.5, 0.5]`: flat mid-grey. |
| Saturation −100 | Every row is `[0.2126, 0.7152, 0.0722]`. |
| Warmth +100 | Red row × 1.15, blue row × 0.85. |

The plan also carries the matrix as the 20 numbers of an SVG `feColorMatrix` of type `matrix`, each rounded to six decimal places:

```text
m0 m1 m2 0 o0   m3 m4 m5 0 o1   m6 m7 m8 0 o2   0 0 0 1 0
```

### Auto

`auto_colour(stats)` sets all four sliders from the photo's own statistics. The statistics are the mean and standard deviation of each of R, G and B, in levels from 0 to 255, measured on the photo's medium thumbnail (sRGB, the whole upright photo, with no edit applied). If the thumbnail is missing it is made first. The whole photo is used, not the crop, so Auto gives the same answer before and after cropping.

```text
auto_colour({ mean: [r, g, b], stdev: [sr, sg, sb] }):
    a value that is not finite counts as 0; a negative stdev counts as 0
    mean_l = 0.2126·r  + 0.7152·g  + 0.0722·b
    spread = 0.2126·sr + 0.7152·sg + 0.0722·sb
    limit(v) = round(clamp(v, −60, 60))

    brightness = limit( (118 − mean_l) / 1.02 )                  // 1.02 = 0.4 × 255 / 100 levels per step
    contrast   = limit( (55 / max(spread, 1) − 1) × 100 )
    warmth     = 0 when r + b < 1, else limit( (b − r) / (0.15 × (r + b)) × 100 )
    saturation = 0
```

Auto aims the overall brightness at level 118 and the spread at 55, and evens out red and blue. No slider moves further than ±60, so Auto stays gentle and can be adjusted afterwards. A black photo, a white photo and a flat photo all give finite values within the limits.

## One implementation, two consumers

The edit model lives in `owlbum-core`. It has two consumers:

- the **renderer** in Rust, which writes edited files and copies edited pictures;
- the **page**, which shows edited photos in tiles, the preview, the viewer and edit mode.

The page does not contain the model. It asks Rust for an **edit plan** and applies it with a CSS transform and an SVG colour filter. It never asks Rust for rendered pixels to show an edit, so an edit appears at once and costs nothing in the cache.

### What Rust owns and what the page owns

| Owned by Rust | Owned by the page |
|---|---|
| Parsing, clamping, identity and equality | The draft edit while edit mode is open |
| `straighten_scale`, `edit_plan`, `colour_matrix`, `auto_colour` | Pointer geometry for the crop box: `resize_crop` and `fit_crop_to_aspect` |
| `rotate_left`, `rotate_right`, `flip` | Multiplying plan numbers by a fit factor |
| Rendering | Slider positions and the aspect choice |

The page's crop geometry only produces a candidate crop in fractions. It cannot break the match between preview and file, because the crop it produces goes through `parse_edit` and `edit_plan` in Rust before anything is drawn from it or saved.

### How the page gets a plan

| Where | How |
|---|---|
| Tiles, preview, viewer | A `Row` (defined in [Organising](05-organising.md#query_run)) and the detail record carry `edit_plan`, the plan for the stored edit, or null. No extra call is made per photo. |
| Edit mode | `edit_plan(photo_id, edit, whole)` returns the plan for a draft. With `whole` set, the crop is ignored, so the plan shows the whole frame. |

In edit mode the page calls `edit_plan` when the draft's turn, flip, straighten or colour changes. At most one call is in flight; if the draft changes again meanwhile, only the newest draft is asked for next. Dragging the crop box makes no call, because the stage shows the whole frame and the box is placed in fractions of it. A plan is arithmetic only and answers in well under a millisecond.

### How the page draws a plan

One component, `EditedImage`, draws every edited photo. It is given a plan and a box of `box_w × box_h` CSS pixels.

```text
contain:  f = min(box_w / out.w, box_h / out.h)       // preview, viewer, edit stage
cover:    f = max(box_w / out.w, box_h / out.h)       // tiles, map thumbnails; centred, the box cuts the overflow

outer element:  width out.w × f, height out.h × f, overflow hidden, position relative
<img>:          position absolute, left 0, top 0, transform-origin 0 0
                width  = pic.w × f
                height = pic.h × f
                transform =
                    translate( (frame.w / 2 − crop.left) × f px,  (frame.h / 2 − crop.top) × f px )
                    rotate( angle deg )
                    scaleX(−1)                                   // only when flip
                    translate( −width / 2 px,  −height / 2 px )
                filter = url(#owlbum-colour-<key>)               // only when colour is not null
```

All numbers are rounded to four decimal places, with no `-0`. The image source is a thumbnail or the full-size upright picture; both are upright and have the photo's shape, so the same plan fits either.

The colour filter is an SVG filter with one `feColorMatrix` of type `matrix` whose `values` are the plan's 20 numbers. The filter must carry `color-interpolation-filters="sRGB"`. Without it the browser applies the matrix in linear light and the preview no longer matches the file. Filters live in one hidden SVG at the page root, one per distinct key in use. CSS filter functions such as `brightness()` and `contrast()` are never used, because their maths is different.

### The test that pins agreement

Three tests together pin "what you see is what gets saved".

1. **Shared plan fixtures.** A Rust test (`owlbum-core`, `edit::fixtures`; run it with `OWLBUM_UPDATE_FIXTURES=1` to write the file again after a deliberate change) writes `fixtures/edit-plans.json`: a list of edits, photo sizes and boxes, each with its plan, the expected `<img>` width, height and transform text, and the expected filter values. The file is committed. The Rust test fails when the model's output differs from the file. A page test reads the same file and asserts that `EditedImage` produces exactly those strings.
2. **Reference preview.** `owlbum-core` has `preview_reference(plan, thumbnail, box)`, a small Rust interpreter of the drawing rules above: the same affine transform with bilinear sampling, then the matrix on 8-bit sRGB values. A Rust test renders each fixture edit twice, once with the renderer at full size scaled down to the box, and once with `preview_reference` on the medium thumbnail. They must agree within a mean absolute difference of 2 levels per channel, with at most 1% of pixels differing by more than 12 levels.
3. **Colour exactness.** For a flat-colour fixture and each fixture colour edit, the renderer's output pixel must be within 1 level per channel of `apply` on the input colour for PNG, and within 3 levels for JPEG.

As built (#150), tests 2 and 3 are in `owlbum_core::edit::reference`. Test 2 renders a smooth generated scene (a blurred `scene` fixture) at a quarter of each fixture's photo size, with a thumbnail that keeps the medium thumbnail's share of the photo (1600 of 3000), so it runs in seconds; `preview_reference` draws the contained box only.

## Where edits are shown

| Place | How the edit shows |
|---|---|
| Grid and timeline tiles | `EditedImage`, cover, on the small thumbnail. A marker shows the photo is edited. |
| Map thumbnails | `EditedImage`'s drawing, cover, written as HTML for the marker (`editedHtml`); each point of `query_map` carries its `edit_plan`. |
| Preview | `EditedImage`, contain, zoomable; the full-size picture is swapped in for zooming with the same plan, and the zoom's mini map draws the small thumbnail through the same plan. A badge names the state. |
| Full-screen viewer | `EditedImage`, contain, with the same zoom as the preview. The viewer always shows the edit. |
| Edit mode | The whole-frame plan, with the crop box over it. |
| Before/after | The plain upright picture, with no plan at all. |

Thumbnails are always made from the unedited file, so an edit never invalidates one. The histogram in the details panel describes the file, not the edit.

### Markers

`photos.edit` means "an edit that is drawn on screen and is not in the file's pixels". What the marker says depends on the `edit_target` setting.

| `edit_target` | Tile marker | Preview and viewer badge | Tooltip |
|---|---|---|---|
| `album` | A dot at the tile's top left, after the Hidden badge, token `--marker-edited` | "Edited" | "Edited. The edit is kept in the album; the file is unchanged." |
| `file` | The same dot, token `--marker-unsaved` | "Edited · not saved" | "Edited, not saved into the file yet" |

Both tokens, the dot's size and place, and the badge's look are defined in [Visual design](09-visual-design.md#badges-on-tiles): `--marker-edited` is a quiet form of the accent and `--marker-unsaved` is the stronger amber. A photo has an edit for the page when its `Row` carries an `edit_plan`. With `edit_target = album` Done is the save, so nothing is ever "not saved"; the words "not saved" appear only with `edit_target = file`.

## Edit mode

Edit mode edits one photo inside the preview pane. It opens with the E key, the **Edit** button in the preview's caption, or **Edit** in the photo menu.

| Rule | Detail |
|---|---|
| What can be edited | Still images and RAW files. Not video, not an animated picture. A file that could not be read (a row with `scan_error`) is in no result, so it can never be chosen. See [Formats](#formats). |
| Starting point | The stored edit, or identity, read from the detail record's `edit` (`photo_detail`) when edit mode opens. The aspect starts at Free, not swapped. |
| One at a time | Opening edit mode on another photo first ends the current one as Done. |
| While a save runs | Edit mode cannot open on a photo that is being saved to its file. |
| The viewer | The viewer cannot open over edit mode. |
| Undo | Ctrl+Z does nothing while edit mode is open. Outside it, each stored change is one undo step in the list [Organising](05-organising.md#undo-ctrlz) keeps. |

### Layout

```text
+----------------------------------------------------------------------------+
| EDIT STAGE (fills the pane; 24 px kept round the frame so handles can be   |
|             grabbed at the edge)                                           |
|      +-------------------------------------------------------------+      |
|      | whole frame: flipped, turned, straightened, colour applied  |      |
|      |   ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░   |      |
|      |   ░░  ■-----------■-----------■  ░░   outside the box is    |      |
|      |   ░░  |     :     |     :     |  ░░   dimmed                |      |
|      |   ░░  ■-----------+-----------■  ░░   rule-of-thirds lines  |      |
|      |   ░░  |     :     |     :     |  ░░   8 handles             |      |
|      |   ░░  ■-----------■-----------■  ░░   drag inside = move    |      |
|      +-------------------------------------------------------------+      |
+----------------------------------------------------------------------------+
| TOOLBAR (wraps onto more rows in a narrow pane)                            |
| [Free|Original|Square|4:5|3:2|16:9] [⇄]  [↺ Rotate left] [↻ Rotate right] [⇋ Flip] |
| Straighten  [-45 ====o==== +45]  +2.5°                                     |
| Brightness  [-100 ===o=== +100]  +10     Contrast  [-100 ===o=== +100]   0 |
| Saturation  [-100 ===o=== +100]  -20     Warmth    [-100 ===o=== +100]  +5 |
| [Auto]                          [Before]      [Reset]  [Cancel]  [Done]    |
+----------------------------------------------------------------------------+
| caption: file name and badges only (no action buttons while editing)       |
+----------------------------------------------------------------------------+
```

### The stage and the crop box

- **Picture.** The medium thumbnail, drawn with the whole-frame plan, fitted into the stage less 24 px on each side, and centred. It is hidden until its size is known.
- **Crop box.** Placed in percentages of the frame. It has a thin light border. Everything outside it is dimmed. Rule-of-thirds lines sit at 33.333% and 66.667%.
- **Handles.** Eight: the four corners and the four sides. Each has an 18 px hit area and an 8 px visible square, the matching resize cursor, and a label such as "Resize from the top left". The box itself has a move cursor. Right-click on the stage does nothing.
- **Dragging.** The pointer is captured. Each move calls `resize_crop(start_crop, handle, dx, dy, ratio, frame)`, with `dx` and `dy` as fractions of the frame.

`resize_crop` rules:

| Case | Rule |
|---|---|
| Always | The box stays inside the frame and is never smaller than 0.05 each way. |
| Move | `x` is clamped to [0, 1 − w] and `y` to [0, 1 − h]; the size is kept. |
| Free, any handle | Each dragged edge is clamped on its own: no closer than 0.05 to the opposite edge, no further than the frame. |
| Fixed ratio, corner | The opposite corner stays put. The new width is the smaller of the dragged width and the dragged height × `rf`, at least `max(0.05, 0.05 × rf)`, and at most what fits between the fixed corner and the frame both ways. The height follows as width ÷ `rf`. |
| Fixed ratio, left or right side | The opposite side stays put. The height follows and stays centred on the old centre. The width is also limited to `2 × min(cy, 1 − cy) × rf`, so the height cannot leave the frame. |
| Fixed ratio, top or bottom side | The same, with width and height exchanged. |

`rf = ratio × frame.h / frame.w`, as in `fit_crop_to_aspect`.

### Toolbar

| Control | Behaviour |
|---|---|
| **Aspect** (segmented group, label "Aspect ratio") | Free, Original, Square, 4:5, 3:2, 16:9. Disabled until the picture's size is known. Choosing one refits the crop. Pressing the active one again changes nothing. |
| **Swap** (icon button, "Swap portrait and landscape") | Enabled for 4:5, 3:2, 16:9, and for Original on a photo that is not square. Flips the ratio and refits. |
| **Rotate left**, **Rotate right** (tooltips "Rotate left ([)" and "Rotate right (])") | `rotate_left` or `rotate_right` on the draft. For 4:5, 3:2 and 16:9 the swap state flips too, so the box turns with the photo. |
| **Flip** (tooltip "Flip horizontally") | `flip` on the draft. |
| **Straighten** | A slider from −45 to 45 in steps of 0.5, followed by the value as "+2.5°", "-3°" or "0°". The crop stays in fractions of the frame. |
| **Brightness**, **Contrast**, **Saturation**, **Warmth** | Sliders from −100 to 100 in steps of 1, each followed by its value as "+10", "-20" or "0". |
| Any slider | A double-click resets it to 0. Its tooltip is "Double-click to reset". Arrow keys move it by one step while it has focus. |
| **Auto** (tooltip "Set light and colour from the photo") | Calls `edit_auto` and sets all four colour sliders. The other parts of the draft are kept. A failure shows "Couldn't work out Auto for this photo." and changes nothing. |
| **Before** | Press and hold to see the original. See [Before and after](#before-and-after). |
| **Reset** | Returns the draft to identity, Free, not swapped. This clears colour too. |
| **Cancel** (tooltip "Esc") | Leaves edit mode. The stored edit is as it was before editing began. |
| **Done** (primary, tooltip "Enter") | Leaves edit mode and stores the draft. |

A mouse click on a toolbar button does not leave focus on it, so Enter afterwards means Done and does not press the button again.

### Keys in edit mode

Keys are ignored with Ctrl, Alt or Meta held, and while typing in a text field. A slider is not a text field, so it keeps its arrow keys.

| Key | Action |
|---|---|
| Esc | Cancel |
| Enter | Done, unless a button has keyboard focus (that button is pressed). Auto-repeat is ignored. |
| `[` | Rotate left. Auto-repeat is ignored. |
| `]` | Rotate right. Auto-repeat is ignored. |
| `\` (hold) | Before |
| `?` | The shortcut overlay (see [Desktop, release and tests](08-desktop-and-release.md)) |

All other review keys do nothing in edit mode.

### Ending edit mode

**Done** calls `edit_set(photo_id, draft)`. If the draft is the same as the stored edit, nothing is written. Otherwise the draft becomes the stored edit (NULL when it is identity), and one undo step labelled "edit" is recorded, holding the edit as it was.

These end edit mode **as Done would**, using the current draft:

- selecting another photo (a click, a key, a map marker, the menu);
- the edited photo leaving the result (filtered out, or its file removed);
- opening another album, or closing this one;
- closing the window or quitting.

If `edit_set` fails, a toast says "Couldn't keep the edit: <reason>". After Done (the button or Enter) edit mode stays open. When it was ending because the photo is no longer shown (another photo selected, the photo left the result, the album left) it closes anyway, since the stage could no longer show that photo.

While edit mode is open, the review screen's own key rows (`browse` in the key table) match nothing, so every other review key, Ctrl+Z, Delete and Ctrl+C included, does nothing; edit mode listens with place `edit`. Closing the window runs the close handshake (`app_closing`, [Desktop, release and tests](08-desktop-and-release.md#closing-and-quitting)): the page ends edit mode as Done before it answers `app_close_ready`, so a draft open when the window closes is kept.

### Before and after

Holding `\`, or pressing and holding the **Before** button, shows the photo with no edit at all, captioned "Original". Releasing shows the edit again. Nothing is changed.

| Rule | Detail |
|---|---|
| Where | The preview, the viewer and edit mode. In edit mode the crop box is hidden while the original shows. |
| The button | At the right end of the preview's caption (before Edit) while the photo has an edit, and in the edit toolbar. It works with the pointer (down to show, up or leave to hide) and with the keyboard (Space or Enter held). |
| Losing focus | If the window loses focus while the key or button is held, the edit shows again. |
| Not while | Typing, or with a dialog or menu open. |

## Rotate and flip outside edit mode

Phones often get the orientation wrong, so turning does not need edit mode.

| Trigger | Action |
|---|---|
| `[` | Rotate left (anticlockwise) |
| `]` | Rotate right (clockwise) |
| Photo menu: **Rotate left**, **Rotate right**, **Flip horizontally** | The same, and flip |

- They act on **every selected photo that is shown**, or on the current photo when nothing else is selected. They work in the grid, the timeline, the map, the preview and the viewer. In the viewer they act on the shown photo only, as star, hide and the nudges do there.
- Each press calls `edit_turn(photo_ids, op)`. Rust applies `rotate_left`, `rotate_right` or `flip` to each photo's stored edit (or to identity) in **one transaction**. An existing crop, straighten and colour are kept; the crop turns with the photo. A result that is identity is stored as NULL.
- One press is **one undo step** for all the photos, labelled "rotate left", "rotate right" or "flip".
- Photos that cannot be edited are skipped. If any were skipped, an info toast says "Skipped N that cannot be edited". If none could be turned, nothing is recorded.
- The keys take no modifiers and ignore auto-repeat. They are not Ctrl+R, which a development build uses to reload the page.
- The result is an ordinary stored edit: it shows the marker and can be saved or reverted like any other.

## Saving

### What Save means

Every edit is kept in the album database the moment it is made. It survives closing the album and quitting. The setting `edit_target` decides what happens next. In Settings it is the group "When I edit a photo" with two radio buttons: "Keep the edit in the album" (`album`) and "Save edits into the photo file" (`file`); the dialog is in [Desktop, release and tests](08-desktop-and-release.md#settings-dialog).

| | `album` (the default) | `file` |
|---|---|---|
| Meaning | Keep the edit in the album. The file is never changed. | Edits are meant to be written into the photo files. |
| After Done | Finished: Done is the save. The photo is "Edited". There is nothing more to save. | The photo is "Edited · not saved" until **Save**. |
| **Save** | Not offered. | Writes the edit into the file. The original goes to the Recycle Bin or Trash. |
| **Save all (N)** | Not offered. | Saves every photo in the album that has an edit. |
| **Save as…** | Offered: writes an edited copy. | Offered. |
| **Revert to original** | Offered: removes the edit. | Offered: removes the edit that is not saved yet. |
| Other programs see | The unedited file. | The unedited file until Save, then the edited one. |

Changing the setting takes effect at once and changes no photo. A failure to store it shows "Couldn't keep the setting: <reason>".

**The count.** `edit_status()` gives `{ edited, waiting, saving }`: the number of photos with a stored edit, the number waiting to be saved (the same number in `file` mode, 0 in `album` mode), and the number being saved now. Rust counts from the database; the page does not report a count.

### Formats

The format registry in [Media and scanning](03-media-and-scanning.md#formats-in-version-1) gives each format its capabilities (`edit_in_place`, `save_as`, `animated`), and the page reads them with `app_formats`. This table repeats the values this part needs; the registry wins if they ever differ.

| Format | Edit mode, rotate, flip | Save into the file | Save as… writes |
|---|---|---|---|
| JPEG | yes | yes, JPEG | JPEG |
| PNG | yes | yes, PNG | PNG |
| WebP (still) | yes | no, copy only | WebP (lossless) |
| TIFF | yes | no, copy only | PNG |
| BMP | yes | no, copy only | PNG |
| GIF (still) | yes | no, copy only | PNG |
| JPEG XL | yes | no, copy only | JPEG |
| Camera RAW | yes | no, copy only | JPEG |
| Animated GIF or WebP | no | no | nothing |
| Video | no | no | nothing |

- A copy-only photo in `file` mode keeps its **Save** item, disabled, with the tooltip "<Label> photos can only be saved as a copy", for example "RAW photos can only be saved as a copy".
- **Edit** on a photo that cannot be edited is disabled with the tooltip "Videos cannot be edited" or "Animated pictures cannot be edited".

### Output

| Setting | Value |
|---|---|
| JPEG | Quality 95, 4:4:4 chroma. Transparent parts are flattened onto white. |
| WebP | Lossless, the only mode the encoder has (see Media and scanning). Transparency kept. This is why a WebP is saved as a copy only. |
| PNG | Lossless. Bit depth and transparency kept. |
| Metadata | The original's EXIF block is kept, with three changes: Orientation is set to 1, the pixel-size tags are set to the new size, and the embedded thumbnail is removed (it would show the old picture). XMP is kept as it is. RAW files give their EXIF to the JPEG copy. As built (#150), see [Metadata as built](#metadata-as-built). |
| Colour profile | See [Rendering](#rendering). |
| File dates | The new file gets the current modified time. The date shown in Owlbum comes from EXIF and does not change. |

### Save into the file: the sequence

`edit_save_to_file(photo_ids)` saves the photos one after another. For each photo:

| Step | What happens | If it fails |
|---|---|---|
| 1. Check | The album is writable; the photo exists, is not missing, has an edit, its format can be saved in place, and it is not already being saved. | `view_only`; "That photo is not in the album any more."; "<name> has no edit to save."; "<Label> photos can only be saved as a copy."; "That photo is already being saved." Nothing is changed. |
| 2. Claim | The photo is marked busy in memory. The scanner and the folder watcher skip its path until the claim is released. | |
| 3. Render | The edit is rendered to `<app data>/renders/<uuid>.<ext>` (see [Rendering](#rendering)). | "Couldn't make the edited photo: <reason>". Nothing is changed. |
| 4. Record | One transaction writes a save record to the album database (below). | "Couldn't save <name>: the album could not be written." The render is deleted. Nothing is changed. |
| 5. Move the original | The original file goes to the Recycle Bin or Trash. | "Couldn't move the original to the <bin>, so nothing was changed: <reason>". The render and the record are deleted. |
| 6. Move the render into place | The render is moved to the original's path (see the move rules below). | "The original of <name> is in the <bin>, but the edited photo could not be put in its place: <reason>. Restore the original from the <bin> and try again." The render stays in `renders/` and the record stays, so the next open can finish the job. |
| 7. Finish | The new file is read like a scan of one file. Then **one transaction** sets `photos.edit` to NULL, updates the row's size, modified time, file key and scan facts, and deletes the save record. As built (#47): the transaction sets the size, modified time, file key and thumbnail key from the new file and the width and height from the render, and clears `analysis`; the scan asked for at step 8 runs the pixels pass, which makes the analysis and the new thumbnails. The camera facts (date, place, camera) are kept from the original, since the file is the same photo; tags are kept too. | "The photo was saved, but Owlbum could not read it again: <reason>". The file is saved; recovery on the next open finishes the row. |
| 8. Release | The claim is released, `album_changed` is emitted with reason `saved` (with the saved photos in `rekeyed`), undo steps that hold this photo's edits are removed (`UndoList::drop_edits_of`), and a scan is asked for. | |

`<bin>` is "Recycle Bin" on Windows and "Trash" on Linux, everywhere in this document.

The row keeps its id, its path and all its typed info and tags through the save. Its thumbnail key changes because the file changed, so new thumbnails are made.

If the photo's edit was changed while its save was running (rotate, undo), the save still clears only the edit it saved: step 7 leaves `photos.edit` alone when it no longer equals the saved edit, and a toast says "A later change to <name> was not saved".

**Move rules** (step 6, and Save as…):

| Rule | Value |
|---|---|
| Normal case | An atomic rename. |
| Busy file | Up to 3 attempts, 200 ms apart, retried only when the system reports the file as busy, in use or access denied (a sync tool, antivirus or a viewer is holding it). |
| Another drive | A rename across drives is not possible. The render is copied to `<target>.<pid>-<ms>.part` beside the target, flushed to disk, renamed onto the target, and the source is deleted. A failure removes the `.part` file. |
| Left-over renders | Files in `renders/` older than one hour are deleted at start, except a render named by a save record of the album being opened. |

**The save record and recovery.** Without a guard, a crash between steps 6 and 7 would leave an edited file with its edit still stored, and the edit would be drawn a second time. The save record prevents that. It is a row in the album database's `meta` table:

```text
key:   save:<photo id>
value: { "edit": <the edit>, "file_key": "<the original's file key>", "render": "<file name in renders/>", "started_at": <ms> }
```

While a photo has a save record, its stored edit is **not drawn**: its `Row` carries no plan. So a file that was replaced is never shown with the edit applied a second time, even when step 7 failed and the program kept running. The scanner settles a record by the table below whenever it meets that photo's path. As built (#47), only the open settles records; while a save holds a path the scanner and the folder watcher skip that path (its row and its file), and the save itself finishes or keeps its record.

On every writable open, each save record is settled before the scan starts:

| What is found at the photo's path | Meaning | Action |
|---|---|---|
| A file with the recorded `file_key` | The save never replaced the file. | Delete the record. The edit stays. |
| A file with another `file_key` | The save replaced the file. | Finish step 7: clear the edit if it still equals the recorded one, refresh the row, delete the record. |
| No file, and the render is still in `renders/` on this computer | The original is in the bin and the move failed. | Do step 6 and step 7. |
| No file and no render | The original is in the bin and the render is gone. | Delete the record, keep the edit, and show: "<name> was being saved when Owlbum stopped. Its original is in the <bin>; restore it to get the photo back." As built (#47) the message goes to the log only; showing it on the page is a follow-up. |

### Save all

`edit_save_all()` saves every photo with an edit, in album order (by path, as built), one after another. It is offered only in `file` mode; in `album` mode it answers `not_supported`. Besides the photo menu, the top bar shows **Save all (N)** in `file` mode while any photo has an edit (N is `edited` from `edit_status`); while a save runs that button shows "Saving 2 of 5…" (or "Saving…") and is disabled.

- Copy-only photos are skipped. Photos already being saved are left to that save.
- `edit_save_progress { done, total, current }` is emitted as each photo starts and once at the end: `done` photos are finished out of `total`, and `current` is the file name of the photo being saved, or null at the end. Failures are in the command's reply, not in the event. The page shows the count in the place of the Save all button.
- It stops after the current photo if the album is closed or the program quits.
- One toast reports the result:

| Outcome | Toast |
|---|---|
| All saved | "Saved N photos" ("Saved 1 photo") |
| Some failed | "Saved N, M failed: a, b, c…" (error; up to three names, then "…") |
| Some skipped | The text above, then ". Skipped K that can only be saved as a copy" |

While any save runs, every Save, Save all and Save as… control is disabled.

### Save as…

`edit_save_as(photo_id)` writes an edited copy and leaves the original and its stored edit as they are. It is offered whenever the photo has an edit, in both modes, and in a view-only album.

| Step | Rule |
|---|---|
| Dialog | Rust opens a native Save dialog titled "Save the edited photo as". It starts in the photo's own folder. The one filter is "<Format label> photo" with that format's extensions. The dialog asks before replacing a file. Cancel does nothing. |
| Default name | `<name>-edited<ext>`. `<ext>` is the photo's own extension when the copy is written in the photo's own format, else the written format's default extension (`.jpg`, `.png`, `.webp`). |
| Extension forcing | The chosen name's extension is forced to fit the written format. A chosen `.jpeg` is kept for JPEG. If forcing changed the name and a file already exists under the new name, the dialog did not ask about it, so the save stops with "A file with that name already exists. Choose another name." |
| The original's own path | Refused with "Choose a different name: Save as keeps the original." The comparison ignores case on Windows. |
| Replacing a file | If the user agreed to replace an existing file, that file is moved to the Recycle Bin or Trash first, never overwritten. A failure stops the save with "Couldn't move the existing file to the <bin>, so nothing was changed: <reason>". |
| Render and move | The same render as Save, then the move rules above. A failure deletes the render and shows "Couldn't save the copy: <reason>". |
| A copy inside the album | The new file is read and gets its own row. The original's typed info is copied to it: star, people, date, place (position and name), description and notes. Not copied: hidden, the score nudge, the edit, and tags (the copy is a different picture and is tagged afresh). In a view-only album nothing is copied. |
| A copy outside the album | Nothing more happens. |
| Success | Toast "Saved a copy as <file name>". |
| Partly done | "The copy was saved as <file name>, but Owlbum could not read it: <reason>", or "The copy was saved as <file name>, but its star, people, date, place, description and notes could not be copied to it: <reason>". |

The Save as… button's tooltip is "Save the edited photo as a new file; this one stays as it is".

### Revert to original

`edit_revert(photo_ids)` sets `photos.edit` to NULL for the photos given, in one transaction and as one undo step labelled "revert to original". Its tooltip is "Remove the edit and show the photo as it is in the file". It cannot bring back a file that Save replaced; that original is in the Recycle Bin or Trash.

### Closing and quitting

Closing loses nothing, because every edit is already in the album. So there is no "unsaved edits" question when opening another album or closing this one.

| Situation | What happens |
|---|---|
| Edit mode is open | It ends as Done first. |
| A save is running | The close waits for the current photo's sequence to finish. Save all stops after that photo. The window shows "Finishing saving…" meanwhile. |
| `album` mode | The window closes. |
| `file` mode with photos waiting | One native question, below. It is asked for closing the window, File > Quit, and before installing an update. |

The question is a native warning box:

| Part | Text |
|---|---|
| Message, one photo | "1 photo has an edit that is not saved into its file." |
| Message, several | "<n> photos have edits that are not saved into their files." |
| Detail | "The edits are kept in the album and will be here next time. Until they are saved, other programs see these photos without them." |
| Buttons | "Close anyway" and "Go back". The default button and Esc are "Go back". |

The question is asked only when `edit_target` is `file`. The shell asks Rust for the count itself (`waiting` from `edit_status`). An update install that already asked does not ask a second time.

**The close handshake.** The draft of an open edit mode lives in the page, so the shell must give the page a chance to store it. When a close or quit is asked for, the shell holds it and emits `app_closing`. The page ends edit mode as Done (waiting for `edit_set`) and then calls `app_close_ready`. If the page has not answered within 3 seconds and no save is running, the shell carries on, so a page that hangs cannot stop the program closing. The handshake itself belongs to [Desktop, release and tests](08-desktop-and-release.md); this part needs only that it exists.

## Rendering

`render_edit(source, edit, target_format, out_path)` in `owlbum-core` makes an edited file. It runs on the blocking pool. The page never calls it directly.

| Step | What it does |
|---|---|
| 1. Decode | Decode the photo at full size through the format registry (RAW through the RAW decoder). Keep 16-bit samples when the source has them. |
| 2. Upright | Apply the EXIF orientation. |
| 3. Plan | `edit_plan(upright.w, upright.h, edit)`. |
| 4. Flip and rotate | Mirror left to right when `plan.flip`, then turn by the quarter turn. Both are exact pixel moves. |
| 5. Straighten | When `straighten` is not 0: each pixel of the frame is sampled from the turned picture, rotated about its centre, with bicubic (Catmull-Rom) sampling. The frame lies wholly inside the picture, so no empty corner is ever sampled. When `straighten` is 0 this step copies pixels unchanged. |
| 6. Crop | Cut `plan.crop` out of the frame. Steps 5 and 6 are done together: only the cropped pixels are computed. |
| 7. Colour | When `plan.colour` is not null: convert the pixels to sRGB, then apply `out = clamp(m · in + offset, 0, 1)` to the sRGB-encoded values. Alpha is untouched. A greyscale picture becomes three channels first. |
| 8. Encode | Encode in the target format with the settings in [Output](#output), with the metadata rules there. |
| 9. Write | Write to `<out_path>.<uuid>.tmp`, flush to disk, and rename onto `out_path`. The temp file is removed on any failure. Returns the written width and height. |

**As built (#43, #47, #150).** `render_edit` decodes through the handler's `source` call, which keeps the file's own bit depth and its ICC profile, and renders in 8 or 16 bits. A 16-bit source saved as PNG stays 16-bit; JPEG and WebP are 8-bit, so a 16-bit source is rounded to 8 bits for them. Grey pictures become RGB. It writes JPEG (quality 95, 4:4:4, transparency on white), PNG or lossless WebP through a temporary file, with the profile by the table below and the metadata by [Output](#output).

#### Metadata as built

`owlbum_core::edit::metadata` is a small pure-Rust EXIF rewriter (no `little_exif`: it could not drop the embedded thumbnail). It reads the original's EXIF and XMP and rewrites the EXIF block in one of two ways:

| Source | EXIF written |
|---|---|
| JPEG (APP1), PNG (`eXIf`), WebP (`EXIF`) | **In place.** The original block's bytes stay where they are, so values stored at an offset, the maker note above all, still read correctly. A new IFD 0 (Orientation 1; ImageWidth and ImageLength set when present) and a new Exif IFD (PixelXDimension and PixelYDimension set, ColorSpace 1 after a colour change) are appended, the header points at the new IFD 0, IFD 1 is unlinked and the thumbnail's bytes are cut off or cleared. |
| TIFF, DNG and the TIFF-based RAW formats | **Compact.** The file is the EXIF structure, so only IFD 0's descriptive tags (description, make, model, resolution, software, date, artist, copyright), the Exif IFD without the maker note (its offsets cannot be moved) and the GPS IFD are copied into a fresh block, with the same three changes. A TIFF's XMP (tag 700) is kept. |
| An in-place block over a JPEG segment's 65,527 bytes | Compact, as above, so the maker note is dropped. |
| RAW formats that are not TIFF-based (CR3, RAF and similar), GIF, BMP, JPEG XL | No EXIF and no XMP. |

XMP is written as it was read: a JPEG APP1 XMP segment (a packet over one segment, extended XMP, is left out), a PNG uncompressed `iTXt` chunk `XML:com.adobe.xmp`, a WebP `XMP ` chunk. Only an uncompressed PNG XMP packet is read.

**Colour profiles.**

| Case | Rule |
|---|---|
| No colour change | Pixels are not converted. The source's ICC profile is embedded in the output unchanged, so a wide-gamut photo stays wide-gamut after a crop or a turn. |
| Colour change | Pixels are converted from the source profile to sRGB before step 7 with `moxcms`, the library the media handlers use (its 16-bit transform for a 16-bit source), the output is tagged sRGB, and the EXIF ColorSpace tag is set to sRGB. This matches the preview, which draws sRGB thumbnails through an sRGB filter. |
| No profile | The source is treated as sRGB. |

**Size limit.** A render needs the whole decoded picture in memory. The limit is the one the media handlers already apply, 300,000,000 pixels (see [Media and scanning](03-media-and-scanning.md#media-handlers)): a larger file is never in a result, so it can never be chosen for editing. If a decode still reports a larger picture, the render answers `edit_render_failed` with "This photo is too large to edit."

## The photo menu

A right-click on a tile, on the preview or in the viewer opens the menu at the pointer. If the photo is not in the selection, it becomes the selection first; otherwise the selection is kept, so the multi-photo items act on all of it. The webview's own context menu is suppressed. The menu has the role `menu` and the label "Photo".

**Targets.** Items marked "all" act on the selected photos that are shown, in the order of the result, when the clicked photo is one of them. Items marked "one" act on the clicked photo only.

```text
+----------------------------------------+
| [crop]     Edit                      E |
| [rotate-l] Rotate left               [ |
| [rotate-r] Rotate right              ] |
| [flip]     Flip horizontally           |
|----------------------------------------|
| [check]    Save                        |   file mode, and the photo has an edit
| [download] Save as…                    |   the photo has an edit
| [check]    Save all (3)                |   file mode, and any photo has an edit
| [undo]     Revert to original          |   a target has an edit
|----------------------------------------|
| [image]    Copy                 Ctrl+C |   "Copy 12 photos" with several
| [copy]     Copy path                   |   "Copy 12 paths" with several
| [folder]   Show in folder              |
|----------------------------------------|
| [star]     Star | Unstar             F |
| [eye-off]  Hide | Unhide             H |
|----------------------------------------|
| [trash]    Move to Recycle Bin     Del |   "Move to Trash" on Linux; danger style
+----------------------------------------+
```

The key hints come from the one shortcut table (see [Desktop, release and tests](08-desktop-and-release.md#every-key)). Star shows F; S does the same thing everywhere. The four save items are hidden while edit mode is open. The menu stays 8 px inside the window.

| Item | Acts on | Disabled when (tooltip) |
|---|---|---|
| **Edit** | one | The photo cannot be edited (the reason from [Formats](#formats)); it is being saved ("This photo is being saved"); view-only. |
| **Rotate left**, **Rotate right**, **Flip horizontally** | all | No target can be edited (the reason from Formats); view-only. |
| **Save** | all targets that have an edit; the label is "Save (N)" when that is more than one | Every such target is copy-only ("<Label> photos can only be saved as a copy"); a save is running ("Saving…"); view-only. Tooltip when enabled: "Save the edit into the photo file (the original goes to the <bin>)". |
| **Save as…** | one | A save is running. |
| **Save all (N)** | every photo in the album with an edit | A save is running; view-only. Tooltip when enabled: "Save every edit into its photo file". |
| **Revert to original** | all targets that have an edit | A save is running; view-only. |
| **Copy** | one: the picture. Several: the files. | Copying files is not supported on this platform and several are selected: the item stays enabled and copies paths (see [Copy several](#copy-several-photos)). |
| **Copy path** | all | Never. |
| **Show in folder** | one | Never. |
| **Star / Unstar**, **Hide / Unhide** | all (rules in [Organising](05-organising.md)) | View-only. |
| **Move to Recycle Bin / Trash** | all | View-only. |

Every view-only tooltip is the first sentence of the view-only banner in force (see [Browsing](04-browsing.md#album-banners)).

**Closing the menu.** Any of these closes it: choosing an item (the menu closes first, then the action runs); a pointer-down outside it; Esc; a review key (the key then acts as usual); a scroll more than 200 ms after it opened; the window losing focus or being resized. A new right-click opens a new menu.

## File actions

### Move to the Recycle Bin or Trash

`file_trash(photo_ids)` is called by the Delete key and by the menu item. It is the only way Owlbum removes a photo, and it never erases.

| Rule | Detail |
|---|---|
| Delete key | In the grid, the timeline, the map and the preview. Not while typing, in a dialog, in a menu, in the viewer or in edit mode. No modifiers; auto-repeat ignored. |
| The viewer | The Delete key is left out of the viewer on purpose, so a stray press while looking through photos full-screen removes nothing. The menu item is still offered there, because choosing it is deliberate. |
| Targets | The selected photos that are shown, else the current photo. Photos hidden by a filter or folded inside a stack are not included. |
| Check | Rust keeps the ids whose files exist. If none is left, nothing happens. |
| Confirm | A native warning box, modal to the window. Rust shows it, so the page cannot skip it. |
| Moving | Files are moved one at a time. A file that cannot be moved stays where it is and is reported. It is never retried another way. |
| A drive with no bin | Some network and removable drives have no Recycle Bin or Trash. The move must fail there; it must never become a permanent delete. On Windows Rust sends a file to the bin only when it is on a local fixed drive; any other file (a network share, a mapped network drive, a removable drive) fails with "This drive has no Recycle Bin." without calling the system. On Linux the `trash` library refuses a mount with no trash folder. |
| The row | Each moved photo's row is marked `missing = 1` in one transaction. It keeps its typed info, its edit and its tags, and it leaves every result. If the file is restored from the bin, the next scan finds it and the row is used again. **Tidy** removes missing rows for good (see [Organising](05-organising.md)). |
| Undo | Not undoable in Owlbum. The bin is the undo. |
| Selection afterwards | Settled like any other change to the result: the page re-runs its query with `keep`, and the tile now in the current photo's place is selected, or the last tile when that place is past the end, or nothing when the result is empty (see [Browsing](04-browsing.md#when-the-result-changes)). Photos that failed to move are still in the result and stay selected. |
| Events | `album_changed` with reason `trashed`. |

The confirm:

| Part | Windows | Linux |
|---|---|---|
| Message, one photo | `Move "<file name>" to the Recycle Bin?` | `Move "<file name>" to the Trash?` |
| Message, several | `Move <n> photos to the Recycle Bin?` | `Move <n> photos to the Trash?` |
| Detail, one photo | "You can restore it from the Recycle Bin." | "You can restore it from the Trash." |
| Detail, several | "You can restore them from the Recycle Bin." | "You can restore them from the Trash." |
| Buttons | "Move to Recycle Bin", "Cancel" | "Move to Trash", "Cancel" |
| Default button and Esc | Cancel | Cancel |

The box is `tauri-plugin-dialog`'s message box with custom buttons. It cannot choose its default button, so the first one ("Move to …") is the default on Windows; Esc still cancels. A follow-up issue tracks making Cancel the default.

Results:

| Outcome | What the user sees |
|---|---|
| Cancelled | Nothing changes. |
| All moved | The tiles go; no toast. |
| Some or all failed | Error toast "Couldn't move <name> to the <bin>: <first reason>" for one failure, or "Couldn't move N photos to the <bin>: <first reason>" for several. The ones that moved are still removed. |
| The command itself failed | "Couldn't delete: <reason>" |

### Show in folder

`file_reveal(photo_id)` opens the system file manager with the photo's file selected. It acts on the clicked photo only. If the file is gone: "That photo's file is not there any more." Any other failure: "Couldn't show the photo: <reason>".

### Copy path

`file_copy_paths(photo_ids)` puts the absolute paths of the target photos on the clipboard as text, one per line, in the order of the result. Rust writes the clipboard; the paths do not pass through the page.

| Rule | Detail |
|---|---|
| Limit | The first 500 photos. |
| Toast | "Copied" for one; "Copied N paths" for several; "Copied the first 500 paths" when more were selected. |
| Failure | "Couldn't copy the path: <reason>" |

### Copy image (one photo)

Ctrl+C with one photo selected, or **Copy** in the menu, calls `file_copy_image(photo_id)`. It puts the **picture** on the clipboard, so it pastes into a chat, a document or an image editor.

| Rule | Detail |
|---|---|
| What is copied | The photo as Owlbum shows it: upright, **with its stored edit applied**, rendered by the renderer. Until the renderer exists (#43) the copy is upright and unedited; the reply's `with_edits` says when the photo has an edit that the copy lacks. |
| Size limit | At most 50 megapixels. A larger picture is scaled down to fit, keeping its shape. |
| Video | The file is copied instead, as in "Copy several". |
| Toast | "Copied"; "Copied, made smaller to fit the clipboard" when it was scaled. |
| Failures | "That photo is not in the album any more."; "Couldn't read that photo."; "Couldn't copy the photo: <reason>" |
| Where the key works | The grid, the timeline, the map, the preview and the viewer. In a text field Ctrl+C stays the field's own. |

### Copy several photos

Ctrl+C with several photos selected calls `file_copy_files(photo_ids)`. It puts the **files** on the clipboard, so they paste into a file manager or into a chat program that takes files.

| Rule | Detail |
|---|---|
| What is copied | The original files, as they are on disk. A stored edit is not in them. |
| Limit | The first 500 photos whose files exist, in the order of the result, each once. |
| Windows | A file drop list, which Explorer and chat programs paste as files. |
| Linux | The `text/uri-list` and `x-special/gnome-copied-files` clipboard formats, which GNOME Files, Nemo and Dolphin paste as files. This is checked by a prototype on X11 and Wayland. Linux work is paused, so version 1 builds use the fallback below on Linux until the prototype is done. |
| Linux fallback | If the prototype cannot make it reliable, Linux copies the paths as text instead, and the reply says so. The toast then never claims files were copied. An issue stays open. |
| Toast, files | "Copied N photos"; "Copied the first 500 photos" when more were selected. |
| Toast, paths (the fallback) | "Copied N paths. Copying files is not supported on Linux yet." |
| Edits left out | When any copied photo has a stored edit, the toast adds " (without their edits)". |
| Failures | "Those photos are not in the album any more."; "Couldn't copy the photos: <reason>" |

## Dragging photos out

Tiles and the preview can be dragged into other programs: a file manager, a mail message, a layout program.

| Rule | Detail |
|---|---|
| What is dragged | The **original files**, never copies. The receiving program copies them: Owlbum offers copy only, never move (the `drag` library offers one effect, and link is left out). A stored edit is not in the files. |
| Which photos | If the dragged photo is selected, the selected photos that are shown, in the order of the result. Otherwise only the dragged photo. |
| Limit | The first 500 whose files exist. |
| Icon | The first photo's small thumbnail scaled to 128 px on its longest side, never enlarged, drawn without its edit. A plain 48 px square in token `--drag-placeholder` when there is no thumbnail. |
| View-only albums | Dragging works. |
| Edit mode | Nothing can be dragged while edit mode is open. |

**Starting the drag.** The operating system starts a drag from where the pointer is at that moment, with the button still down. A drag that first waits for work can miss a quick flick. So the work is done early:

1. On pointer-down (primary button) on a tile or the preview, the page calls `file_prepare_drag(photo_ids)`. Rust resolves the ids, checks the files exist, loads the icon, and keeps the result for that exact list of ids, in that order, for 10 seconds.
2. When the pointer has moved 5 px with the button down, the page calls `file_start_drag(photo_ids)`. The page does not use the webview's own drag, so an image is never dragged as a web picture.
3. Rust uses the prepared result when it is for the same list and at most 10 seconds old. Otherwise it prepares now (it does not wait for a preparation still in flight).
4. Just before the drag starts, the files are checked for existence once more. If none is left, nothing happens.

Rust starts the drag from the shell crate. `tauri-plugin-drag` offers the page a command that takes file paths, which the id rule forbids, so that command is **not** granted to the page. The shell needs a Rust-side way to start the same drag; see the open questions.

| Failure | Message |
|---|---|
| The files are gone | "Couldn't drag the photos: the files are not there any more." |
| The system refused the drag | "Couldn't drag the photos: <reason>" |

After a drag or a file copy that included photos with a stored edit, an info toast says, once per session: "Photos are dragged as they are on disk, without edits kept in the album. Use Save as… for an edited copy."

**Linux.** A prototype checks drag-out on Linux before the feature is built: X11 and Wayland, into GNOME Files, Dolphin, a browser upload field and one chat program, with 1, 20 and 500 files. If it cannot be made reliable, drag-out ships on Windows only. On Linux no drag starts, the shortcut overlay lists dragging as "Windows only for now", and an issue stays open. The page learns this from `app_capabilities() → { drag_out, copy_files }`, which the shell answers, not from the user agent.

## View-only

| Works in a view-only album | Disabled |
|---|---|
| Seeing stored edits everywhere; before/after | Edit mode |
| Save as… (typed info is not copied to the copy) | Rotate, flip |
| Copy image, copy files, copy path | Save, Save all, Revert to original |
| Show in folder | Move to the Recycle Bin or Trash |
| Dragging photos out | |

Moving to the bin is disabled because a view-only album is either a folder that cannot be written or an album another computer is using.

## Commands and events

Every command takes photo ids. An unknown id answers `photo_not_found`. A command that changes the album answers `view_only` in a view-only album.

| Command | Arguments | Reply | Notes |
|---|---|---|---|
| `edit_plan` | `photo_id, edit, whole` | `EditPlan` | Pure. `edit_invalid` for a bad edit. |
| `edit_transform` | `edit, op` | `PhotoEdit` | `op` is `rotate_left`, `rotate_right` or `flip`. Pure; used by edit mode's buttons. |
| `edit_auto` | `photo_id` | `Colour` | Reads the medium thumbnail. |
| `edit_set` | `photo_id, edit or null` | `{ changed, plan, revision }` | Done. Identity stores NULL. Records an undo step. `plan` is the plan of the stored edit now (null for none); the grid's re-run on `album_changed` brings the new `Row`. |
| `edit_turn` | `photo_ids, op` | `{ changed, skipped, revision }` | One transaction, one undo step. |
| `edit_revert` | `photo_ids` | `{ changed, revision }` | One transaction, one undo step. |
| `edit_status` | | `{ edited, waiting, saving }` | |
| `edit_save_to_file` | `photo_ids` | `{ saved: [id], failed: [{ id, name, message }], skipped, warnings, revision }` | The sequence above, one photo after another. `warnings` holds "A later change to <name> was not saved". |
| `edit_save_all` | | the same | `file` mode only (`not_supported` in `album` mode). |
| `edit_save_as` | `photo_id` | `{ cancelled, saved_name, in_album, photo_id, warning, revision }` | Rust opens the dialog. One flat shape: `cancelled` true leaves the rest empty. |
| `file_trash` | `photo_ids` | `{ trashed: [id], failed: [{ id, name, message }], cancelled, revision }` | Rust shows the confirm. |
| `file_reveal` | `photo_id` | nothing | |
| `file_copy_image` | `photo_id` | `{ scaled, file, with_edits }` | `file` when a video's file was copied instead; `with_edits` while the copy cannot show the edit yet. |
| `file_copy_files` | `photo_ids` | `{ copied, dropped, as, with_edits }` | `as` is `files` or `paths`. |
| `file_copy_paths` | `photo_ids` | `{ copied, dropped }` | |
| `file_prepare_drag` | `photo_ids` | nothing | Never fails; a failure shows at start. |
| `file_start_drag` | `photo_ids` | `{ dragged, with_edits }` | |
| `app_capabilities` | | `{ drag_out, copy_files }` | What this platform offers. `copy_files` is `files` or `paths`. |
| `app_close_ready` | | nothing | The page's answer to `app_closing`. Defined in [Desktop, release and tests](08-desktop-and-release.md#commands-and-events-this-part-adds). |

| Event | Payload | When |
|---|---|---|
| `edit_save_progress` | `{ done, total, current }` | While a save of more than one photo runs: as each photo starts, and once at the end. |
| `album_changed` | reason `edit`, `saved` or `trashed` | An edit was stored or removed; a file was saved; photos were moved to the bin. |
| `app_closing` | `{}` | The window is about to close or the program to quit. Defined in Architecture and Desktop; this part answers it with `app_close_ready`. |

Error codes this part adds to `AppError`:

| Code | Meaning |
|---|---|
| `edit_invalid` | The edit sent by the page did not parse. |
| `edit_not_editable` | Video or an animated picture. |
| `edit_copy_only` | The format cannot be saved in place. |
| `edit_none` | The photo has no edit to save. |
| `edit_busy` | The photo is already being saved. |
| `edit_render_failed` | The edited photo could not be made. |
| `edit_original_not_moved` | The original could not go to the bin; nothing was changed. |
| `edit_not_placed` | The original is in the bin and the render could not be moved into place. |
| `edit_saved_not_read` | The file was saved and could not be read again. |
| `save_as_same_file` | The target is the original. |
| `save_as_exists` | The forced extension names an existing file. |
| `file_missing` | The photo's file is not there. |
| `trash_failed` | The command could not run at all. Per-file failures are in the reply. |
| `clipboard_failed` | The clipboard could not be written. |
| `drag_failed` | The drag could not start. |
| `not_supported` | The platform does not offer this action. |

## Defects of the old program that this design fixes

| Old behaviour | Owlbum |
|---|---|
| The full-screen viewer drew the unedited picture. | Every surface draws through `EditedImage`; the viewer shows edits. |
| Unsaved edits lived in the page's memory and were lost on close, behind a warning. | Every edit is in the album database at once. Closing loses nothing. |
| The page told the shell how many edits were unsaved. | Rust counts from the database. |
| A crash between writing the file and clearing the edit drew the edit twice. | The save record and its recovery on open. |
| Save as… could overwrite an existing file. | The existing file goes to the bin first. |
| The embedded EXIF thumbnail kept showing the picture before the edit. | It is removed on save. |
| The confirm for one photo said "restore them". | "restore it" for one photo. |
| Copying several files worked on Windows only and said nothing elsewhere. | A stated Linux method, a stated fallback, and a toast that says which happened. |
| Dragging and copying gave unedited files with no notice. | A toast says so, and points to Save as…. |
| Any photo ever catalogued could be dragged. | Only photos of the open album can be named, because commands take ids. |

## Tests

### Rust tests (`owlbum-core`)

| Area | Tests |
|---|---|
| Parsing | Each row of the parsing table; `{"rotate":0}` gives identity; `flip: "yes"` is invalid; brightness 500 becomes 100; a crop of width 0 becomes 0.01; unknown fields are dropped; the stored form round-trips. |
| Identity and keys | `is_identity` is false for each single non-zero field; `edit_key` differs for flip on and off and for each colour value; equal edits share a key. |
| Geometry | `straighten_scale` is 1 at 0° and matches hand-worked values at 10° and 45° for 3:2 and 1:1; `edit_plan` for a 3000 × 2000 photo at each quarter turn; the crop clamps at the frame edge; the output is never 0 wide. |
| Turning | `rotate_left` four times, `rotate_right` four times and `flip` twice are the identity operation; `rotate_right(rotate_left(e)) = e`; a crop in the top-left stays on the same part of the picture after each operation (checked by rendering). |
| Colour | The five rows of the check table; `colour_matrix` of all zeros is exactly identity; the SVG values have 20 numbers with the alpha row `0 0 0 1 0`. |
| Auto | A dark photo gives brightness above 0; a flat photo gives contrast above 0; a blue cast gives warmth above 0; a balanced photo (mean 118, spread 55, equal channels) gives all 0; all-black and all-white give finite values within ±60; saturation is always 0. |
| Agreement | The three tests in [The test that pins agreement](#the-test-that-pins-agreement). |

**Pixel-level render tests** run on generated fixtures, never on personal photos:

| Fixture and edit | Assertion |
|---|---|
| 4 × 2 PNG, left half red; flip | Red is on the right. |
| 4 × 2 PNG with one marked corner; each quarter turn | The marked corner lands where the turn puts it, and the size is swapped for 90 and 270. |
| 400 × 200 PNG of quadrant colours; crop `{0.5, 0, 0.5, 0.5}` | Output is 200 × 100 and one colour. |
| The EXIF-rotated trip photo; identity edit plus a crop | The output is upright, its Orientation tag is 1, its pixel-size tags match, and it has no embedded thumbnail. |
| A photo with camera EXIF; any edit | Camera, lens, date and GPS tags are equal before and after. |
| 8 × 8 mid-grey JPEG; brightness +50 | The mean rises by 51 levels, within 3. |
| A red PNG; saturation −100 | R, G and B are equal, within 1. |
| A wide-gamut PNG with an ICC profile; crop only | The profile bytes are unchanged. The same with a colour edit: the output is tagged sRGB. |
| A PNG with transparency; saved as JPEG | Transparent parts are white. |
| A 16-bit PNG; crop | The output is 16-bit. |
| A straighten of 10° on a photo with a solid border | No pixel of the output is outside the source picture (no black corner). |
| The small DNG; Save as | A JPEG is written; the DNG is byte-for-byte unchanged. |

**Save and file tests** use a temp album and a stand-in bin:

| Test | Assertion |
|---|---|
| Save into the file | The original is in the bin, the path holds the render, `photos.edit` is NULL, the row id and typed info are the same, and the file key changed. |
| Each failing step | The message in the sequence table, and the state it promises (for example nothing changed when the bin refuses). |
| Busy target | A target that fails twice as busy and then succeeds is saved; one that fails three times reports the step 6 message. |
| Another drive | The copy-and-rename path leaves no `.part` file on success or failure. |
| Recovery | Each of the four rows of the recovery table, by building the state by hand and opening the album. The edit is never applied twice. |
| Scanner during a save | A scan started between steps 5 and 6 does not mark the row missing and does not make a second row. |
| Save as | The default name; extension forcing; refusing the original's path, with different case on Windows; an existing target goes to the bin; typed info is copied to a copy inside the album and nothing is copied outside it. |
| Copy-only and not editable | Each row of the formats table gives the stated result. |
| Trash | Rows become missing and keep their typed info; a file that fails stays and is reported; restoring the file and rescanning brings the row back; a cancelled confirm changes nothing. |
| Limits | 600 ids give 500 and `dropped: 100` for copy files, copy path and drag; duplicates count once; missing files are left out. |
| Drag preparation | A prepared list is used for the same ids in the same order within 10 seconds, and not for another order or after 10 seconds. |
| View-only | Every changing command answers `view_only` and changes nothing. |
| Turn many | `edit_turn` on three photos writes three edits in one transaction and records one undo step; a photo with a crop keeps it, turned. |

### Page tests (faked API)

| Area | Tests |
|---|---|
| `EditedImage` | The shared plan fixtures give the exact width, height, transform and filter values, for contain and cover. The filter element has `color-interpolation-filters="sRGB"`. |
| Crop geometry | `resize_crop` for every handle, free and fixed ratio; the 0.05 minimum; the box never leaves the frame. `fit_crop_to_aspect` for each preset, and swapped. |
| Edit mode | Dragging a slider asks for a plan with at most one call in flight; double-click resets a slider; Auto fills the four sliders from the fake's reply; Reset gives identity; Enter is Done after a mouse click on Rotate; Esc cancels and sends nothing; selecting another photo sends `edit_set` with the draft. |
| Before/after | Holding `\` draws the plain picture with the caption "Original"; releasing restores the edit; a window blur restores it; the draft is unchanged. |
| Rotate keys | `]` with three photos selected calls `edit_turn` once with three ids; in a text field it types a bracket. |
| Markers | The dot and badge follow `edit_target`; the viewer draws through `EditedImage`. |
| Menu | The items and their order; each disabled state and tooltip from the table; the save items are hidden in edit mode and in `album` mode as specified; the labels follow the count; each closing rule. |
| Delete key | Calls `file_trash` with the shown, selected ids; moves the selection to the next photo from the reply; does nothing in a text field, the viewer or edit mode. |
| Ctrl+C | One photo calls `file_copy_image`; several call `file_copy_files`; each toast text, including the 500 limit, the paths fallback and "(without their edits)". |
| Dragging | Pointer-down calls `file_prepare_drag`; a 5 px move calls `file_start_drag` with the same ids; a 3 px move does not; a failure shows the message. |
| Capabilities | With drag-out reported as not supported, no drag call is made. |

## Open questions

1. **Is the close question in `file` mode wanted at all?** Nothing is lost by closing, so it is only a reminder. It could be dropped, or shown once per session.
2. **Does the trash library ever delete for good?** On a Windows drive with no Recycle Bin, a recycle request can turn into a permanent delete unless the right flags are set. A prototype must show that `trash` fails there instead, on Windows and on Linux mounts with no trash folder. If it cannot, the shell needs its own call.
3. **File lists on the Linux clipboard.** Which mechanism works on both X11 and Wayland, and whether the clipboard keeps the list after the window loses focus, is not known until the prototype.
4. **Writing metadata.** Settled in #150: a small pure-Rust rewriter instead of `little_exif`; see [Metadata as built](#metadata-as-built).
5. **Colour management.** Settled: the renderer decodes through the handlers' `source` call and converts with `moxcms` (8- and 16-bit transforms) by the [Colour profiles](#rendering) table (#150).
6. **SVG filters on many tiles in WebKitGTK.** A grid in which most tiles carry a colour filter and a transform may scroll slowly on Linux. If it does, tiles with a colour edit need small rendered thumbnails from Rust instead.
7. **TIFF in place.** TIFF is copy-only in version 1 because writing TIFF with its metadata intact is unproven. Scans kept as TIFF would be better served by saving as TIFF.
8. **Lossless JPEG turns.** A save whose edit is only a quarter turn or a flip could turn the JPEG without re-encoding. Version 1 re-encodes at quality 95.
9. **Starting a drag from Rust.** It is assumed, not yet confirmed, that the library under `tauri-plugin-drag` can be called from the shell crate with a list of paths, and that it can offer copy and link without move. The Linux prototype checks this on both platforms.
10. **A file changed by another program while it has a stored edit.** The edit is kept and drawn on the new content. Whether to warn is undecided.
