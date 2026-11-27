// Text shown by the edit feature (src/features/edit).
// docs/06-editing-and-file-actions.md, "Rotate and flip outside edit mode".

export const editText = {
  /** Why a photo cannot be edited (docs/06, "Formats"). */
  video: "Videos cannot be edited",
  animated: "Animated pictures cannot be edited",
  /** The info toast after a turn that skipped videos or animated pictures. */
  skipped: (n: number) => `Skipped ${n} that cannot be edited`,
};
