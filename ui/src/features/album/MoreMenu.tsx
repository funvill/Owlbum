import { useCallback, useEffect, useState } from "react";
import { api, toAppError } from "../../api";
import type { UpdateState } from "../../api/bindings/UpdateState";
import { MenuButton, showToast, type MenuEntry } from "../../controls";
import { matchKey, primaryKey, type KeyId } from "../../keys";
import { moreText as t } from "../../text/app";
import { openAbout } from "../app/aboutOpen";
import { openWhatsNew, useLifecycle } from "../app/lifecycle";
import { toggleShortcuts, type Where } from "../app/shortcuts";
import { viewOnlyReason } from "./banners";
import { toggleDetails, toggleGrid, toggleGridPlace, usePanels } from "./panels";
import { useAlbumStore } from "./session";
import { useScanStore } from "./store";
import { TidyDialog } from "./TidyDialog";

/** Development-only items show only in a development build. */
const DEV_BUILD = import.meta.env.DEV;

/** Runs a command of the shell; a failure raises a toast. */
function shell(
  command: "app_open_data_folder" | "app_full_screen" | "app_developer_tools" | "app_update_choose" | "app_open_feedback" | "app_quit",
): void {
  void api()
    .invoke(command)
    .catch((e: unknown) => showToast({ kind: "error", message: toAppError(e).message }));
}

/** Whether an album is open. */
const albumOpen = () => useAlbumStore.getState().status !== null;

/**
 * What each key of the old menu bar does (owner's feedback, issue 197):
 * with no native menu, the page handles them, from every place.
 */
export const APP_KEY_ACTIONS: Partial<Record<KeyId, () => void>> = {
  "app.open_folder": () => void useAlbumStore.getState().pickFolder(),
  "app.close_folder": () => {
    if (albumOpen()) void useAlbumStore.getState().close();
  },
  "app.full_screen": () => shell("app_full_screen"),
  "app.about": openAbout,
  "app.feedback": () => shell("app_open_feedback"),
  "app.quit": () => shell("app_quit"),
  "app.reload": () => window.location.reload(),
  "app.developer_tools": () => shell("app_developer_tools"),
};

/** Whether a modal layer other than a full-page surface (the viewer) is open. */
function dialogOpen(): boolean {
  return document.querySelector('[aria-modal="true"]:not([data-keys="surface"])') !== null;
}

/**
 * Listens for the old menu bar's keys while mounted: Ctrl+O, Ctrl+W,
 * Ctrl+Q, F1, F2, F11, and in a development build Ctrl+R and
 * Ctrl+Shift+I. They act everywhere, while typing too, but not over a
 * dialog. `place` says where the user is now.
 */
export function useAppKeys(place: () => Where): void {
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      const match = matchKey(e, { place: place(), focus: dialogOpen() ? "dialog" : undefined });
      const act = match === null ? undefined : APP_KEY_ACTIONS[match.id];
      if (match === null || act === undefined) return;
      e.preventDefault();
      if (match.act) act();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [place]);
}

/** The update item's label, and why it cannot be chosen now (docs/08, "States"). */
export function updateItem(u: UpdateState): { label: string; busy: boolean } {
  const version = u.version ?? "";
  switch (u.state) {
    case "checking":
      return { label: t.update.checking, busy: true };
    case "available":
      return { label: t.update.download(version), busy: false };
    case "downloading":
      return { label: t.update.downloading(u.percent ?? 0), busy: true };
    case "ready":
      return { label: t.update.install(version), busy: false };
    case "installing":
      return { label: t.update.installing, busy: true };
    default:
      return { label: t.update.check, busy: false };
  }
}

/**
 * The More menu (⋯) beside the Settings gear (docs/08-desktop-and-release.md,
 * "The More menu"; owner's feedback, issue 197). It holds what the menu bar
 * and the Tidy button did: the folder commands, Tidy, the panels, full
 * screen, the data folder, help and Quit. Each item shows its key.
 */
export function MoreMenu() {
  const status = useAlbumStore((s) => s.status);
  const panels = usePanels();
  const update = useLifecycle((s) => s.update);
  const [tidying, setTidying] = useState(false);
  const openTidy = useCallback(() => setTidying(true), []);
  const noAlbum = status === null ? t.noAlbum : undefined;
  // Tidy is off in a view-only album; the reason says why.
  const tidyOff = status === null ? t.noAlbum : (viewOnlyReason(status) ?? undefined);
  const item = updateItem(update);
  const key = (id: KeyId) => primaryKey(id);

  const entries: MenuEntry[] = [
    { id: "open_folder", label: t.openFolder, icon: "folder", shortcut: key("app.open_folder"), onSelect: APP_KEY_ACTIONS["app.open_folder"]! },
    {
      id: "close_folder",
      label: t.closeFolder,
      icon: "close-album",
      shortcut: key("app.close_folder"),
      disabledReason: noAlbum,
      onSelect: APP_KEY_ACTIONS["app.close_folder"]!,
    },
    {
      id: "rescan",
      label: t.rescan,
      icon: "rescan",
      disabledReason: noAlbum,
      onSelect: () => void useScanStore.getState().rescan(),
    },
    { id: "tidy", label: t.tidy, icon: "tidy", disabledReason: tidyOff, onSelect: openTidy },
    "separator",
    { id: "thumbnails", label: t.thumbnails, checked: panels.grid, shortcut: key("browse.toggle_grid"), onSelect: () => void toggleGrid() },
    { id: "details", label: t.details, checked: panels.details, shortcut: key("browse.toggle_details"), onSelect: () => void toggleDetails() },
    { id: "thumbnails_bottom", label: t.thumbnailsBottom, checked: panels.place === "bottom", onSelect: () => void toggleGridPlace() },
    { id: "full_screen", label: t.fullScreen, shortcut: key("app.full_screen"), onSelect: APP_KEY_ACTIONS["app.full_screen"]! },
    ...(DEV_BUILD
      ? ([
          { id: "reload", label: t.reload, shortcut: key("app.reload"), onSelect: APP_KEY_ACTIONS["app.reload"]! },
          {
            id: "developer_tools",
            label: t.developerTools,
            shortcut: key("app.developer_tools"),
            onSelect: APP_KEY_ACTIONS["app.developer_tools"]!,
          },
        ] satisfies MenuEntry[])
      : []),
    "separator",
    { id: "data_folder", label: t.dataFolder, onSelect: () => shell("app_open_data_folder") },
    "separator",
    { id: "shortcuts", label: t.shortcuts, icon: "keyboard", shortcut: key("app.shortcuts"), onSelect: toggleShortcuts },
    { id: "whats_new", label: t.whatsNew, onSelect: () => void openWhatsNew() },
    {
      id: "update",
      label: item.label,
      disabledReason: item.busy ? t.update.busy : undefined,
      onSelect: () => shell("app_update_choose"),
    },
    { id: "feedback", label: t.feedback, shortcut: key("app.feedback"), onSelect: APP_KEY_ACTIONS["app.feedback"]! },
    { id: "about", label: t.about, shortcut: key("app.about"), onSelect: openAbout },
    "separator",
    { id: "quit", label: t.quit, shortcut: key("app.quit"), onSelect: APP_KEY_ACTIONS["app.quit"]! },
  ];

  return (
    <>
      <MenuButton label={t.open} icon="more" entries={entries} />
      <TidyDialog open={tidying} onClose={() => setTidying(false)} />
    </>
  );
}
