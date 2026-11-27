// Before and after (docs/06-editing-and-file-actions.md, "Before and
// after"): holding `\`, or pressing and holding a Before button, shows the
// photo with no edit at all. Releasing shows the edit again; nothing is
// changed. The preview, the viewer and edit mode read `held`.

import { useEffect } from "react";
import { create } from "zustand";
import { isRelease, type KeyId } from "../../keys";

export const useBeforeStore = create<{ held: boolean }>(() => ({ held: false }));

/** Shows the original until `releaseBefore`. */
export function holdBefore(): void {
  if (!useBeforeStore.getState().held) useBeforeStore.setState({ held: true });
}

/** Shows the edit again. */
export function releaseBefore(): void {
  if (useBeforeStore.getState().held) useBeforeStore.setState({ held: false });
}

/** The rows that hold Before, one per place. */
export const BEFORE_KEY_IDS = ["browse.before", "viewer.before", "edit.before"] as const satisfies readonly KeyId[];

/**
 * Releases Before on the key's keyup and when the window loses focus,
 * wherever the hold started. Mounted once with the review screen.
 */
export function useBeforeRelease(): void {
  useEffect(() => {
    const onKeyUp = (e: KeyboardEvent) => {
      if (isRelease(e, "browse.before")) releaseBefore();
    };
    window.addEventListener("keyup", onKeyUp, true);
    window.addEventListener("blur", releaseBefore);
    return () => {
      window.removeEventListener("keyup", onKeyUp, true);
      window.removeEventListener("blur", releaseBefore);
      releaseBefore();
    };
  }, []);
}

/** Pointer and keyboard handlers for a Before button: down (or Space/Enter held) shows, up or leave hides. */
export const beforeButtonHandlers = {
  onPointerDown: (e: { button: number }) => {
    if (e.button === 0) holdBefore();
  },
  onPointerUp: releaseBefore,
  onPointerLeave: releaseBefore,
  onPointerCancel: releaseBefore,
  onKeyDown: (e: { key: string; repeat: boolean; preventDefault(): void }) => {
    if (e.key !== " " && e.key !== "Enter") return;
    e.preventDefault();
    holdBefore();
  },
  onKeyUp: (e: { key: string }) => {
    if (e.key === " " || e.key === "Enter") releaseBefore();
  },
  onBlur: releaseBefore,
};
