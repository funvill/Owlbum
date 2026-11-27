// Dragging photos out (docs/06-editing-and-file-actions.md, "Dragging
// photos out"). The system starts a drag from where the pointer is while
// the button is down, so the work is done early: pointer-down prepares,
// and a 5 px move starts the drag from Rust. The webview's own drag is
// never used.

import { api, toAppError } from "../../api";
import { showToast } from "../../controls/toastQueue";
import { noteEditsNotInFiles } from "./actions";
import { loadCapabilities, useCapabilities } from "./capabilities";
import { dragTargets } from "./targets";

/** How far the pointer moves with the button down before a drag starts. */
export const DRAG_START_PX = 5;

interface Press {
  x: number;
  y: number;
  /** Down to the 5 px move or the release. */
  stop: () => void;
}

let press: Press | null = null;

/** The press has ended without a drag, or the drag started. */
function end(): void {
  press?.stop();
  press = null;
}

async function start(ids: number[]): Promise<void> {
  try {
    const out = await api().invoke("file_start_drag", { photo_ids: ids });
    noteEditsNotInFiles(out.with_edits);
  } catch (e) {
    showToast({ kind: "error", message: toAppError(e).message });
  }
}

/**
 * A primary-button press on the photo `id` (a tile or the preview). Does
 * nothing where the platform does not offer drag-out.
 */
export function pressPhoto(id: number, event: { button: number; clientX: number; clientY: number }): void {
  end();
  if (event.button !== 0) return;
  const known = useCapabilities.getState().capabilities;
  if (known === null) void loadCapabilities();
  if (known?.drag_out !== true) return;
  const ids = dragTargets(id);
  void api()
    .invoke("file_prepare_drag", { photo_ids: ids })
    .catch(() => {});
  const onMove = (e: PointerEvent) => {
    if (press === null) return;
    if ((e.buttons & 1) === 0) {
      end();
      return;
    }
    if (Math.hypot(e.clientX - press.x, e.clientY - press.y) < DRAG_START_PX) return;
    end();
    void start(ids);
  };
  const onUp = () => end();
  window.addEventListener("pointermove", onMove);
  window.addEventListener("pointerup", onUp);
  window.addEventListener("pointercancel", onUp);
  press = {
    x: event.clientX,
    y: event.clientY,
    stop: () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onUp);
    },
  };
}

/** For tests: no press held. */
export function resetDrag(): void {
  end();
}
