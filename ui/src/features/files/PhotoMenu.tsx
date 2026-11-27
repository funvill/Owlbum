import { useEffect } from "react";
import { Menu, type MenuEntry } from "../../controls";
import type { Point } from "../../controls/place";
import { primaryKey } from "../../keys";
import { filesText } from "../../text/files";
import { useAlbumStore, viewOnlyReason } from "../album";
import { gridBlocks, useGridStore } from "../grid";
import { toggleFlag } from "../organise";
import { bin, copyPaths, copyPhotos, revealPhoto, trashPhotos } from "./actions";
import { selectedTargets } from "./targets";

const t = filesText.menu;

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
 * copy, copy path, show in folder, star, hide and move to the bin. The
 * edit items (Edit, rotate, flip, the save items) join it with edit mode
 * and saving.
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
