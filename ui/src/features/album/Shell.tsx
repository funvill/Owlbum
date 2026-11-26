import { useEffect } from "react";
import { AlbumBanners } from "./AlbumBanners";
import { Body } from "./Body";
import { OpenQuestions } from "./OpenQuestions";
import { ReadingBar } from "./ReadingBar";
import { useAlbumStore } from "./session";
import { TopBar } from "./TopBar";
import "./Shell.css";

/**
 * The whole window, top to bottom (docs/04-browsing.md, "Whole window"):
 * the top bar, the album banners, the reading bar and the body, with the
 * questions opening asks over them. The tagging bar, notices (#51) and
 * the filter bar come with their issues.
 */
export function Shell() {
  useEffect(() => {
    void useAlbumStore.getState().start();
  }, []);

  return (
    <div className="ow-shell">
      <TopBar />
      <AlbumBanners />
      <ReadingBar />
      <main className="ow-shell__body">
        <Body />
      </main>
      <OpenQuestions />
    </div>
  );
}
