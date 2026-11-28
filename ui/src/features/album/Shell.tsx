import { useCallback, useEffect } from "react";
import { AboutDialog } from "../app/AboutDialog";
import { closeAbout, useAboutOpen } from "../app/aboutOpen";
import { useRegionKeys } from "../app/regions";
import { useShortcutKey, type Where } from "../app/shortcuts";
import { SettingsDialog } from "../app/SettingsDialog";
import { closeSettings, useSettingsOpen } from "../app/settingsOpen";
import { ShortcutOverlay } from "../app/ShortcutOverlay";
import { useEditStore } from "../edit/mode";
import { useGridStore } from "../grid";
import { useViewerStore } from "../viewer/store";
import { FilterBar } from "../organise";
import { AlbumBanners } from "./AlbumBanners";
import { Body, bodyState } from "./Body";
import { Notices } from "./Notices";
import { OpenQuestions } from "./OpenQuestions";
import { ReadingBar } from "./ReadingBar";
import { useAlbumStore } from "./session";
import { TaggingBar } from "./TaggingBar";
import { TopBar } from "./TopBar";
import "./Shell.css";

/**
 * The whole window, top to bottom (docs/04-browsing.md, "Whole window"):
 * the top bar, the album banners, the reading bar, the tagging bar, the
 * notices, the filter bar (while an album with photos is open) and the body,
 * with the questions opening asks over them.
 */
export function Shell() {
  const review = useAlbumStore((s) => bodyState(s) === "review");
  const editing = useEditStore((s) => s.photoId !== null);
  const viewing = useViewerStore((s) => s.open);
  const view = useGridStore((s) => s.query.view);
  const settingsOpen = useSettingsOpen((s) => s.open);
  const aboutOpen = useAboutOpen((s) => s.open);
  const here: Where = editing ? "edit" : viewing ? "viewer" : "browse";
  // `?` opens the shortcut overlay from every place (docs/08, "The overlay").
  useShortcutKey(
    useCallback(
      (): Where =>
        useEditStore.getState().photoId !== null ? "edit" : useViewerStore.getState().open ? "viewer" : "browse",
      [],
    ),
  );
  // F6 and Shift+F6 move between the regions (docs/09, "Accessibility baseline").
  useRegionKeys();
  useEffect(() => {
    void useAlbumStore.getState().start();
  }, []);

  return (
    <div className="ow-shell">
      <TopBar />
      <AlbumBanners />
      <ReadingBar />
      <TaggingBar />
      <Notices />
      {review && <FilterBar />}
      <main className="ow-shell__body">
        <Body />
      </main>
      <OpenQuestions />
      <ShortcutOverlay here={here} view={view} />
      <SettingsDialog open={settingsOpen} onClose={closeSettings} />
      <AboutDialog open={aboutOpen} onClose={closeAbout} />
    </div>
  );
}
