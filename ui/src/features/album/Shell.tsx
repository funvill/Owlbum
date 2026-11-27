import { useEffect } from "react";
import { FilterBar } from "../organise";
import { AlbumBanners } from "./AlbumBanners";
import { Body, bodyState } from "./Body";
import { Notices } from "./Notices";
import { OpenQuestions } from "./OpenQuestions";
import { ReadingBar } from "./ReadingBar";
import { useAlbumStore } from "./session";
import { TopBar } from "./TopBar";
import "./Shell.css";

/**
 * The whole window, top to bottom (docs/04-browsing.md, "Whole window"):
 * the top bar, the album banners, the reading bar, the notices, the filter
 * bar (while an album with photos is open) and the body, with the questions
 * opening asks over them. The tagging bar comes with its issue.
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
      <Notices />
      {review && <FilterBar />}
      <main className="ow-shell__body">
        <Body />
      </main>
      <OpenQuestions />
    </div>
  );
}
