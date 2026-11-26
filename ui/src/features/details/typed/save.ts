// Saving typed info (docs/04-browsing.md, "How typed fields save").
//
// Saves for one photo are sent one after another, on a chain kept outside
// the panel, so a save sent as the panel moves to another photo (leaving
// without blur) still runs in order after the ones before it.

import { useCallback, useEffect, useRef, useState } from "react";
import { api, toAppError } from "../../../api";
import type { InfoChanged } from "../../../api/bindings/InfoChanged";
import type { InfoPatch } from "../../../api/bindings/InfoPatch";
import { showToast } from "../../../controls/toastQueue";

/** How long a field's label shows "Saved". */
export const SAVED_MS = 1800;

const chains = new Map<number, Promise<unknown>>();

/** Sends `patch` for photo `id` after every save already on its way for it. */
export function sendInfo(id: number, patch: InfoPatch): Promise<InfoChanged> {
  const before = chains.get(id) ?? Promise.resolve();
  const next = before.then(
    () => api().invoke("photo_set_info", { ids: [id], patch }),
    () => api().invoke("photo_set_info", { ids: [id], patch }),
  );
  chains.set(id, next);
  const done = () => {
    if (chains.get(id) === next) chains.delete(id);
  };
  next.then(done, done);
  return next;
}

/** The field keys that show "Saved". */
export type FieldKey = "description" | "notes" | "people" | "date" | "location";

export interface InfoSaver {
  /** Sends `patch`; true when it was saved. */
  save(field: FieldKey, patch: InfoPatch): Promise<boolean>;
  /** The field whose label shows "Saved", or null. */
  saved: FieldKey | null;
  /** The message of the last failed save, until a later save succeeds. */
  error: string | null;
}

/** The panel's saver for photo `id`. */
export function useInfoSaver(id: number): InfoSaver {
  const [saved, setSaved] = useState<FieldKey | null>(null);
  const [error, setError] = useState<string | null>(null);
  const mounted = useRef(true);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      clearTimeout(timer.current);
    };
  }, []);

  const save = useCallback(
    async (field: FieldKey, patch: InfoPatch) => {
      try {
        await sendInfo(id, patch);
        if (mounted.current) {
          setError(null);
          setSaved(field);
          clearTimeout(timer.current);
          timer.current = setTimeout(() => setSaved(null), SAVED_MS);
        }
        return true;
      } catch (e) {
        const message = toAppError(e).message;
        showToast({ kind: "error", message });
        if (mounted.current) setError(message);
        return false;
      }
    },
    [id],
  );

  return { save, saved, error };
}
