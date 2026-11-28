// Text shown by the grid feature (src/features/grid).

export const gridText = {
  label: "Photos",
  /** The name of a tile whose row has not arrived. */
  loading: "Loading…",
  noMatch: "No photos match the filter",
  /** The note over the photos found by meaning. */
  alsoMeaning: "Also matching by meaning",
  clearFilter: "Clear filter",
  thumbSize: "Thumbnail size",
  star: "Star",
  unstar: "Unstar",
  starred: "Starred",
  hidden: "Hidden",
  score: (n: number) => `Score ${n}`,
  noScore: "Score not worked out yet",
  editedAlbum: "Edited. The edit is kept in the album; the file is unchanged.",
  editedFile: "Edited, not saved into the file yet",
  stackShow: (n: number) => `Show ${n} similar photos`,
  stackFold: (n: number) => `Fold ${n} similar photos`,
  /** "IMG_0042.jpg, starred, score 55, stack of 3" (docs/09-visual-design.md, "Accessibility baseline"). */
  tileName: (t: { fileName: string; star: boolean; hidden: boolean; score: number | null; stack: number | null; edited: boolean }) =>
    [
      t.fileName,
      t.star ? "starred" : null,
      t.hidden ? "hidden" : null,
      t.score === null ? "score not worked out yet" : `score ${t.score}`,
      t.stack === null ? null : `stack of ${t.stack}`,
      t.edited ? "edited" : null,
    ]
      .filter((part) => part !== null)
      .join(", "),
};
