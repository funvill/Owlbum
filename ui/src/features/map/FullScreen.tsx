// Full screen for a map (owner's feedback, issue 210; docs/04-browsing.md,
// "Map view"): a button over the map makes it fill the window, and the
// button again or Esc puts it back.
//
// The map covers the window with CSS rather than the browser's Fullscreen
// API: the photo menu, the viewer, dialogs and toasts are drawn outside the
// map, and an element in browser full screen would hide them. F11 makes the
// window itself full screen.

import { useEffect, useState, type RefObject } from "react";
import { IconButton } from "../../controls";
import { mapText as t } from "../../text/map";

/** Whether a menu, or a dialog that is not the one holding the map, has Esc. */
function escTakenAbove(frame: HTMLElement | null): boolean {
  if (document.querySelector('[role="menu"]') !== null) return true;
  return [...document.querySelectorAll('[aria-modal="true"]')].some((m) => frame === null || !m.contains(frame));
}

/**
 * Whether the map fills the window, and its switch. While it does, Esc (when
 * no menu or other dialog has it) puts it back.
 */
export function useFullScreen(frame: RefObject<HTMLElement | null>) {
  const [full, setFull] = useState(false);
  useEffect(() => {
    if (!full) return;
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || escTakenAbove(frame.current)) return;
      e.preventDefault();
      e.stopPropagation();
      setFull(false);
    };
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [full, frame]);
  return { full, toggle: () => setFull((f) => !f) };
}

/** The button over the map's bottom right corner. */
export function FullScreenButton({ full, onToggle }: { full: boolean; onToggle(): void }) {
  return (
    <div className="ow-map__full">
      <IconButton
        icon={full ? "exit-full-screen" : "full-screen"}
        label={full ? t.exitFullScreen : t.fullScreen}
        shortcut={full ? "Escape" : undefined}
        pressed={full}
        onClick={onToggle}
      />
    </div>
  );
}
