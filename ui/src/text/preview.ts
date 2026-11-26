// Text shown by the preview feature (src/features/preview).
// docs/04-browsing.md, "Preview pane".

export const previewText = {
  region: "Preview",
  hint: "Select a photo to see it here.",
  /** The stage while the current photo's row has not arrived. */
  loading: "Loading…",
  selected: (n: number) => `${n} selected ·`,
  starred: "Starred",
  hidden: "Hidden",
  inStack: (place: number, size: number) => `${place} of ${size} in stack`,
  editedAlbum: "Edited",
  editedFile: "Edited · not saved",
  miniMap: "Where the view is in the photo",
};
