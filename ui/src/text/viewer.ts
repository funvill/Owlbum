// Text shown by the viewer feature (src/features/viewer).
// docs/04-browsing.md, "Full-screen viewer".

export const viewerText = {
  /** The dialog's label while the shown photo's row is not held yet. */
  region: "Viewer",
  position: (index: number, count: number) => `${index + 1} / ${count}`,
  keyHint: "← → move · S or F star · H hide · I details · Esc close",
  close: "Close",
  closeShortcut: "Esc",
  starred: "Starred",
  hidden: "Hidden",
  editedAlbum: "Edited",
  editedFile: "Edited · not saved",
  details: "Details",
  taken: "Taken",
  camera: "Camera",
  size: "Size",
  length: "Length",
  people: "People",
  description: "Description",
  sizeValue: (width: number, height: number, size: string) => `${width} × ${height} px · ${size}`,
};
