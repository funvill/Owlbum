// Text shown by the edit feature (src/features/edit).
// docs/06-editing-and-file-actions.md, "Edit mode", "Before and after" and
// "Rotate and flip outside edit mode".

export const editText = {
  /** Why a photo cannot be edited (docs/06, "Formats"). */
  video: "Videos cannot be edited",
  animated: "Animated pictures cannot be edited",
  /** The info toast after a turn that skipped videos or animated pictures. */
  skipped: (n: number) => `Skipped ${n} that cannot be edited`,

  /** Edit mode (docs/06, "Edit mode"). */
  region: "Edit photo",
  stage: "Photo being edited",
  toolbar: "Edit tools",
  crop: "Crop",
  handles: {
    nw: "Resize from the top left",
    ne: "Resize from the top right",
    sw: "Resize from the bottom left",
    se: "Resize from the bottom right",
    n: "Resize from the top",
    s: "Resize from the bottom",
    w: "Resize from the left",
    e: "Resize from the right",
  },
  aspect: "Aspect ratio",
  aspects: {
    free: "Free",
    original: "Original",
    square: "Square",
    "4:5": "4:5",
    "3:2": "3:2",
    "16:9": "16:9",
  },
  swap: "Swap portrait and landscape",
  swapUnavailable: "Choose Original, 4:5, 3:2 or 16:9 to swap",
  rotateLeft: "Rotate left",
  rotateRight: "Rotate right",
  flip: "Flip horizontally",
  straighten: "Straighten",
  brightness: "Brightness",
  contrast: "Contrast",
  saturation: "Saturation",
  warmth: "Warmth",
  resetSlider: "Double-click to reset",
  /** A slider's value: "+2.5°", "-3°", "0°"; "+10", "-20", "0". */
  degrees: (v: number) => `${signed(v)}°`,
  amount: (v: number) => signed(v),
  auto: "Auto",
  autoTooltip: "Set light and colour from the photo",
  autoFailed: "Couldn't work out Auto for this photo.",
  before: "Before",
  reset: "Reset",
  cancel: "Cancel",
  done: "Done",
  keepFailed: (reason: string) => `Couldn't keep the edit: ${reason}`,

  /** Saving (docs/06, "Saving" and "The photo menu"). */
  save: (n: number) => (n > 1 ? `Save (${n})` : "Save"),
  saveTooltip: (bin: string) => `Save the edit into the photo file (the original goes to the ${bin})`,
  saveAs: "Save as…",
  saveAsTooltip: "Save the edited photo as a new file; this one stays as it is",
  saveAll: (n: number) => `Save all (${n})`,
  saveAllTooltip: "Save every edit into its photo file",
  revert: "Revert to original",
  revertTooltip: "Remove the edit and show the photo as it is in the file",
  saving: "Saving…",
  savingProgress: (done: number, total: number) => `Saving ${done + 1} of ${total}…`,
  copyOnly: (label: string) => `${label} photos can only be saved as a copy`,
  beingSaved: "This photo is being saved",
  saved: (n: number) => (n === 1 ? "Saved 1 photo" : `Saved ${n} photos`),
  savedSomeFailed: (saved: number, names: readonly string[]) =>
    `Saved ${saved}, ${names.length} failed: ${names.slice(0, 3).join(", ")}${names.length > 3 ? "…" : ""}`,
  skippedCopyOnly: (k: number) => `. Skipped ${k} that can only be saved as a copy`,
  savedCopy: (name: string) => `Saved a copy as ${name}`,
};

/** A number with its sign: "+2.5", "-3", "0". */
function signed(v: number): string {
  if (v === 0) return "0";
  return v > 0 ? `+${v}` : String(v);
}
