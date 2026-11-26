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
