// Text shown by the organise feature (src/features/organise).
// docs/05-organising.md, "Star and hide", "Sorting", "The filter bar" and "Undo".

/** A whole number with digit grouping: 2,140. */
function group(n: number): string {
  return n.toLocaleString("en-US");
}

export const organiseText = {
  /** The filter bar (docs/05-organising.md, "The filter bar"). */
  bar: {
    label: "Sort and filter",
    sort: "Sort",
    /** The Filters button: "Filters", or "Filters · N" while N panel filters are on. */
    filters: (n: number) => (n > 0 ? `Filters · ${n}` : "Filters"),
    /** "2,140 photos" while no filter is on. */
    count: (n: number) => (n === 1 ? "1 photo" : `${group(n)} photos`),
  },
  /** The filtered row (docs/05-organising.md, "The filtered row"). */
  filtered: {
    label: "Active filters",
    showing: (x: number, y: number) => `Showing ${group(x)} of ${group(y)}`,
    clear: "Clear filter",
    remove: (chip: string) => `Remove filter ${chip}`,
    removeTip: "Remove this filter",
  },
  /** The filter panel (docs/05-organising.md, "The filter panel"). */
  panel: {
    label: "Filters",
    groups: { photo: "Photo", when: "When", where: "Where", content: "Content", quality: "Quality" },
    stars: "Stars",
    starOptions: { all: "All", yes: "Starred", no: "Not starred" },
    shape: "Shape",
    shapeOptions: { any: "Any", portrait: "Portrait", landscape: "Landscape" },
    hidden: "Hidden",
    hiddenOptions: { leave: "Leave out", show: "Show", only: "Only" },
    from: "From",
    to: "To",
    folder: "Folder",
    allFolders: "All folders",
    hasLocation: "Has location",
    hasPeople: "Has people",
    lowQuality: "Low quality",
    nearDuplicates: "Near-duplicates",
    noContent: "Pick a keyword under What's in it, or type tag:word in the search box.",
    /** An option with its facet: "Starred (312)"; no count while none is known. */
    counted: (label: string, n: number | undefined) => (n === undefined ? label : `${label} (${group(n)})`),
  },
  /** The Sort control's options, in their order (docs/05-organising.md, "Sorting"). */
  sorts: {
    date: "Date taken",
    score: "Photo score",
    interesting: "Most interesting",
    colour: "Main colour",
    name: "File name",
  },
  /** The mark that plays after a key changed a photo's star or hidden flag. */
  mark: {
    starred: "Starred",
    unstarred: "Star removed",
    hidden: "Hidden",
    unhidden: "Shown again",
  },
  /** The score toast when no target changed (docs/05-organising.md, "Nudges"). */
  nudgeLimit: "Can't nudge any further",
  /** "4 photos" in the score toast. */
  photos: (n: number) => (n === 1 ? "1 photo" : `${n} photos`),
  /** The undo toasts (docs/05-organising.md, "What `album_undo` does"). */
  undo: {
    nothing: "Nothing to undo",
    done: (label: string) => `Undid: ${label}`,
    partial: (label: string) => `Couldn't undo all of: ${label}`,
    gone: (label: string) => `Couldn't undo ${label}: the photo is no longer in the album`,
  },
  /** The score card (docs/05-organising.md, "The score card"). */
  card: {
    title: "Photo score",
    reading: "still reading the photo",
    breakdown: "What made this score",
    total: "Total",
    footer: "Up and Down nudge it by 1. Star +50, hidden −50.",
    notAnalysed: "Focus, exposure and colour are added once the photo has been read.",
  },
};
