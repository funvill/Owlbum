// How long things wait or stay, in milliseconds. These are behaviour, not
// animation; animation durations are the --motion-* tokens.
// docs/09-visual-design.md, "Tooltips" and "Toasts".

/** The pointer rests this long on a control before its tooltip shows. */
export const tooltipDelayMs = 500;

/** How long a toast stays. Errors stay until dismissed. */
export const toastMs = {
  plain: 4000,
  withAction: 8000,
} as const;

/** The viewer's top bar fades out after this long without pointer movement or a key. */
export const viewerBarMs = 3000;

/** How long the star or hide mark stays over the photo (docs/05-organising.md, "The mark"). */
export const markMs = 800;

/** How long the score toast stays after a nudge (docs/05-organising.md, "The score toast"). */
export const scoreToastMs = 1100;
