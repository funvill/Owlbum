import { useEffect } from "react";
import { Menu, type MenuEntry } from "../../controls";
import type { Point } from "../../controls/place";
import { primaryKey } from "../../keys";
import { filesText } from "../../text/files";
import { useAlbumStore, viewOnlyReason } from "../album";
import { BLOCK, gridBlocks, useGridStore } from "../grid";
import { turn } from "../edit/turn";
import { notEditable, openEdit } from "../edit/mode";
import { toggleFlag } from "../organise";
import { bin, copyPaths, copyPhotos, revealPhoto, trashPhotos } from "./actions";
import { selectedTargets } from "./targets";

const t = filesText.menu;

/** The row of photo `id` among the rows the grid holds. */
function rowOf(id: number) {
  const blocks = gridBlocks();
  for (const b of blocks.held()) {
    for (let i = b * BLOCK; i < (b + 1) * BLOCK; i++) {
      const row = blocks.row(i);
      if (row?.id === id) return row;
    }
  }
  return undefined;
}

/** A scroll this long after the menu opened closes it (one that opening caused does not). */
const SCROLL_GRACE_MS = 200;

export interface PhotoMenuProps {
  /** Where the right-click was. */
  at: Point;
  /** The photo right-clicked; already in the selection. */
  photoId: number;
  onClose: () => void;
}

/**
 * The photo menu (docs/06-editing-and-file-actions.md, "The photo menu"):
 * edit, rotate and flip, copy, copy path, show in folder, star, hide and
 * move to the bin. The save items join it with saving (#47).
 */
export function PhotoMenu({ at, photoId, onClose }: PhotoMenuProps) {
  const status = useAlbumStore((s) => s.status);
  const viewOnly = status === null ? null : viewOnlyReason(status);
  const targets = selectedTargets();
  const all = targets.includes(photoId) ? targets : [photoId];
  const current = useGridStore.getState().current;
  const row = current === null ? undefined : gridBlocks().row(current.index);
  const starred = row?.star ?? false;
  const hidden = row?.hidden ?? false;
  const blocked = viewOnly ?? undefined;
  // Rotate and flip need a target that can be edited; the reason comes
  // from the first that cannot (docs/06-editing-and-file-actions.md, "Formats").
  const why = (id: number): string | null => {
    const r = rowOf(id);
    return r === undefined ? null : notEditable(r);
  };
  const reasons = all.map(why);
  const turnBlocked = blocked ?? (reasons.every((r) => r !== null) ? (reasons[0] ?? undefined) : undefined);

  // The window losing focus or changing size, or a later scroll, closes it.
  useEffect(() => {
    const opened = performance.now();
    const onScroll = () => {
      if (performance.now() - opened > SCROLL_GRACE_MS) onClose();
    };
    window.addEventListener("blur", onClose);
    window.addEventListener("resize", onClose);
    window.addEventListener("scroll", onScroll, true);
    window.addEventListener("wheel", onScroll, true);
    return () => {
      window.removeEventListener("blur", onClose);
      window.removeEventListener("resize", onClose);
      window.removeEventListener("scroll", onScroll, true);
      window.removeEventListener("wheel", onScroll, true);
    };
  }, [onClose]);

  const entries: MenuEntry[] = [
    {
      id: "edit",
      label: t.edit,
      icon: "crop",
      shortcut: primaryKey("browse.edit"),
      disabledReason: blocked ?? why(photoId) ?? undefined,
      onSelect: () => void openEdit(photoId),
    },
    {
      id: "rotate-left",
      label: t.rotateLeft,
      icon: "rotate-ccw",
      shortcut: primaryKey("browse.rotate_left"),
      disabledReason: turnBlocked,
      onSelect: () => void turn("rotate_left", "selection", all),
    },
    {
      id: "rotate-right",
      label: t.rotateRight,
      icon: "rotate-cw",
      shortcut: primaryKey("browse.rotate_right"),
      disabledReason: turnBlocked,
      onSelect: () => void turn("rotate_right", "selection", all),
    },
    { id: "flip", label: t.flip, icon: "flip-h", disabledReason: turnBlocked, onSelect: () => void turn("flip", "selection", all) },
    "separator",
    {
      id: "copy",
      label: t.copy(all.length),
      icon: "image",
      shortcut: primaryKey("browse.copy"),
      onSelect: () => void copyPhotos(all),
    },
    { id: "copy-path", label: t.copyPath(all.length), icon: "copy", onSelect: () => void copyPaths(all) },
    { id: "reveal", label: t.reveal, icon: "folder", onSelect: () => void revealPhoto(photoId) },
    "separator",
    {
      id: "star",
      label: starred ? t.unstar : t.star,
      icon: "star",
      shortcut: primaryKey("browse.star"),
      disabledReason: blocked,
      onSelect: () => void toggleFlag("star", "selection"),
    },
    {
      id: "hide",
      label: hidden ? t.unhide : t.hide,
      icon: "eye-off",
      shortcut: primaryKey("browse.hide"),
      disabledReason: blocked,
      onSelect: () => void toggleFlag("hidden", "selection"),
    },
    "separator",
    {
      id: "trash",
      label: t.trash(bin()),
      icon: "trash",
      shortcut: primaryKey("browse.trash") === "Delete" ? "Del" : primaryKey("browse.trash"),
      danger: true,
      disabledReason: blocked,
      onSelect: () => void trashPhotos(all),
    },
  ];

  return <Menu label={t.label} at={at} entries={entries} onClose={onClose} />;
}
