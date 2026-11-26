// Text shown by the organise feature (src/features/organise).
// docs/05-organising.md, "Star and hide", "Sorting" and "The filter bar".

export const organiseText = {
  /** The filter bar (docs/05-organising.md, "The filter bar"). */
  bar: {
    label: "Sort and filter",
    sort: "Sort",
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
