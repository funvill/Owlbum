// Regenerate's state (docs/07-tagging.md, "Regenerate"): the photos whose
// new description is being written, kept while the section is folded,
// another photo is shown or the panel is closed, and the tags each reply
// brought back.

import { create } from "zustand";
import { api, toAppError } from "../../api";
import type { PhotoTags } from "../../api/bindings/PhotoTags";
import { showToast } from "../../controls";
import { useAlbumStore } from "../album";

export interface RegenerateState {
  /** Photo ids with a Regenerate running. */
  busy: number[];
  /** The tags a reply brought, with the detail revision they replace. */
  replies: Record<number, { tags: PhotoTags; revision: number }>;
}

export const useRegenerateStore = create<RegenerateState>(() => ({ busy: [], replies: {} }));

/** Which album is open: a reply for another album is dropped. */
const albumKey = () => {
  const s = useAlbumStore.getState().status;
  return s === null ? null : `${s.root}|${s.scope}`;
};

/**
 * Calls `tagging_regenerate(id)` unless one already runs for the photo.
 * `revision` is that of the detail shown, which the reply replaces until a
 * newer detail arrives.
 */
export async function regenerate(id: number, revision: number): Promise<void> {
  if (useRegenerateStore.getState().busy.includes(id)) return;
  useRegenerateStore.setState((s) => ({ busy: [...s.busy, id] }));
  const album = albumKey();
  try {
    const tags = await api().invoke("tagging_regenerate", { id });
    if (albumKey() !== album) return;
    useRegenerateStore.setState((s) => ({ replies: { ...s.replies, [id]: { tags, revision } } }));
  } catch (e) {
    if (albumKey() === album) showToast({ kind: "error", message: toAppError(e).message });
  } finally {
    useRegenerateStore.setState((s) => ({ busy: s.busy.filter((b) => b !== id) }));
  }
}

/** Forgets everything. For tests. */
export function resetRegenerate(): void {
  useRegenerateStore.setState({ busy: [], replies: {} });
}
