import { useEffect } from "react";
import { FilterBar } from "../organise";
import { AlbumBanners } from "./AlbumBanners";
import { Body, bodyState } from "./Body";
import { OpenQuestions } from "./OpenQuestions";
import { ReadingBar } from "./ReadingBar";
import { useAlbumStore } from "./session";
import { TopBar } from "./TopBar";
import "./Shell.css";

/**
 * The whole window, top to bottom (docs/04-browsing.md, "Whole window"):
 * the top bar, the album banners, the reading bar, the filter bar (while
 * an album with photos is open) and the body, with the questions opening
 * asks over them. The tagging bar and notices (#51) come with their issues.
 */
export function Shell() {
  const review = useAlbumStore((s) => bodyState(s) === "review");
  useEffect(() => {
    void useAlbumStore.getState().start();
  }, []);

  return (
    <div className="ow-shell">
      <TopBar />
      <AlbumBanners />
      <ReadingBar />
      {review && <FilterBar />}
      <main className="ow-shell__body">
        <Body />
      </main>
      <OpenQuestions />
    </div>
  );
}
