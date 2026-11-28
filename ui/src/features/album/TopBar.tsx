import { SaveAllButton } from "../edit/SaveAllButton";
import { Button, IconButton, Pill, Tooltip, useNarrowWindow } from "../../controls";
import { albumText } from "../../text/album";
import { settingsText } from "../../text/app";
import { openSettings } from "../app/settingsOpen";
import { ThumbSize } from "../grid";
import { FiltersButton } from "../organise/FiltersButton";
import { QueryBox } from "../organise/QueryBox";
import { viewOnlyBanner, viewOnlyReason } from "./banners";
import { bodyState } from "./Body";
import { MoreMenu } from "./MoreMenu";
import { useAlbumStore } from "./session";

/**
 * The bar at the top of the window (docs/04-browsing.md, "Top bar"). While
 * an album with photos is open the search box sits at its left with the
 * Filters button beside it, in line with Open folder and Close folder at
 * the right; the album's folder is in the window title, not here (owner's
 * feedback, issues 182 and 196). The More menu and the gear that opens
 * Settings are always there; Tidy is in the More menu (issue 197).
 */
export function TopBar() {
  const status = useAlbumStore((s) => s.status);
  const review = useAlbumStore((s) => bodyState(s) === "review");
  const openFolder = useAlbumStore((s) => s.openFolder);
  const pick = useAlbumStore((s) => s.pickFolder);
  const close = useAlbumStore((s) => s.close);
  // Below 1100 px the buttons become icon buttons (docs/09-visual-design.md, "The bars").
  const narrow = useNarrowWindow();
  // Save all is off in a view-only album; the tooltip says why.
  const viewOnly = status === null ? undefined : (viewOnlyReason(status) ?? undefined);
  const banner = status === null ? null : viewOnlyBanner(status);
  const scoped = status !== null && status.scope !== "";

  return (
    <header className="ow-topbar" data-region="top">
      <div className="ow-topbar__left">
        {review && <QueryBox />}
        {review && <FiltersButton />}
        {scoped && (
          <button type="button" className="ow-topbar__whole" onClick={() => void openFolder(status.root)}>
            {albumText.showWholeAlbum}
          </button>
        )}
        {banner !== null && (
          <Tooltip text={banner.text}>
            <span className="ow-topbar__badge" tabIndex={0}>
              <Pill>{albumText.viewOnlyBadge}</Pill>
            </span>
          </Tooltip>
        )}
      </div>
      {status !== null && (
        <div className="ow-topbar__right">
          <SaveAllButton viewOnly={viewOnly} />
          <ThumbSize />
          {narrow ? (
            <>
              <IconButton icon="close-album" label={albumText.closeFolder} onClick={() => void close()} />
              <IconButton icon="folder" label={albumText.openFolder} onClick={() => void pick()} />
            </>
          ) : (
            <>
              <Button icon="close-album" onClick={() => void close()}>
                {albumText.closeFolder}
              </Button>
              <Button icon="folder" onClick={() => void pick()}>
                {albumText.openFolder}
              </Button>
            </>
          )}
        </div>
      )}
      <div className="ow-topbar__settings">
        <MoreMenu />
        <IconButton icon="settings" label={settingsText.open} onClick={() => openSettings()} />
      </div>
    </header>
  );
}
