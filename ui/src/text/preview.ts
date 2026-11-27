// Text shown by the preview feature (src/features/preview).
// docs/04-browsing.md, "Preview pane".

export const previewText = {
  region: "Preview",
  hint: "Select a photo to see it here.",
  selected: (n: number) => `${n} selected ·`,
  starred: "Starred",
  hidden: "Hidden",
  inStack: (place: number, size: number) => `${place} of ${size} in stack`,
  editedAlbum: "Edited",
  editedFile: "Edited · not saved",
  miniMap: "Where the view is in the photo",
  /** Before/after: the caption over the unedited picture (docs/06). */
  original: "Original",
  /** The caption's edit actions (docs/06-editing-and-file-actions.md, "Edit mode"). */
  edit: "Edit",
  editShortcut: "E",
  before: "Before",
  beforeTooltip: "Hold to see the original (\\)",
};
