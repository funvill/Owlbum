// Text shown by the files feature (src/features/files): the photo menu,
// trash, copy and dragging out. docs/06-editing-and-file-actions.md,
// "The photo menu", "File actions" and "Dragging photos out".
// Messages Rust writes (failures, the native confirm) are in
// crates/owlbum-core/src/text/files.rs.

/** A whole number with digit grouping: 2,140. */
function group(n: number): string {
  return n.toLocaleString("en-US");
}

/** The bin's name on this platform. */
export function binName(platform: string): string {
  return platform === "windows" ? "Recycle Bin" : "Trash";
}

export const filesText = {
  menu: {
    label: "Photo",
    copy: (n: number) => (n > 1 ? `Copy ${group(n)} photos` : "Copy"),
    copyPath: (n: number) => (n > 1 ? `Copy ${group(n)} paths` : "Copy path"),
    reveal: "Show in folder",
    edit: "Edit",
    rotateLeft: "Rotate left",
    rotateRight: "Rotate right",
    flip: "Flip horizontally",
    star: "Star",
    unstar: "Unstar",
    hide: "Hide",
    unhide: "Unhide",
    trash: (bin: string) => `Move to ${bin}`,
  },
  toast: {
    copied: "Copied",
    copiedScaled: "Copied, made smaller to fit the clipboard",
    copiedPaths: (copied: number, dropped: number) =>
      dropped > 0 ? `Copied the first ${group(copied)} paths` : copied === 1 ? "Copied" : `Copied ${group(copied)} paths`,
    copiedFiles: (copied: number, dropped: number) =>
      dropped > 0 ? `Copied the first ${group(copied)} photos` : `Copied ${group(copied)} photos`,
    /** The fallback where files cannot be put on the clipboard. */
    copiedPathsInstead: (copied: number) =>
      `Copied ${group(copied)} paths. Copying files is not supported on Linux yet.`,
    withoutEdits: " (without their edits)",
    trashFailedOne: (bin: string, name: string, reason: string) => `Couldn't move ${name} to the ${bin}: ${reason}`,
    trashFailedMany: (bin: string, n: number, reason: string) =>
      `Couldn't move ${group(n)} photos to the ${bin}: ${reason}`,
    /** Once per session, after a drag or a file copy that included edited photos. */
    editsNotInFiles:
      "Photos are dragged as they are on disk, without edits kept in the album. Use Save as… for an edited copy.",
  },
  /** The trash question's refusing button, its default (Rust writes the rest). */
  trashCancel: "Cancel",
  /** The shortcut overlay's note where dragging out is not offered. */
  dragWindowsOnly: "Windows only for now",
};
