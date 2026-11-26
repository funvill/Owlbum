// Text shown by the organise feature (src/features/organise).
// docs/05-organising.md, "Star and hide".

export const organiseText = {
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
