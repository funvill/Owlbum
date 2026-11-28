import { useState } from "react";
import { SaveAllButton } from "../edit/SaveAllButton";
import { Button, IconButton, Pill, Tooltip, useNarrowWindow } from "../../controls";
import { albumText } from "../../text/album";
import { appText, settingsText } from "../../text/app";
import { openSettings } from "../app/settingsOpen";
import { ThumbSize } from "../grid";
import { QueryBox } from "../organise/QueryBox";
import { viewOnlyBanner, viewOnlyReason } from "./banners";
import { bodyState } from "./Body";
import { TidyDialog } from "./TidyDialog";
import { useAlbumStore } from "./session";

/**
 * The bar at the top of the window (docs/04-browsing.md, "Top bar"). While
 * an album with photos is open the search box sits at its left, in line
 * with Open folder and Close folder at the right; the album's folder is in
 * the window title, not here (owner's feedback, issue 182). The gear that
 * opens Settings is always there.
 */
export function TopBar() {
  const status = useAlbumStore((s) => s.status);
  const review = useAlbumStore((s) => bodyState(s) === "review");
  const openFolder = useAlbumStore((s) => s.openFolder);
  const pick = useAlbumStore((s) => s.pickFolder);
  const close = useAlbumStore((s) => s.close);
  // Below 1100 px the buttons become icon buttons (docs/09-visual-design.md, "The bars").
  const narrow = useNarrowWindow();
  const [tidying, setTidying] = useState(false);
  // Tidy is off in a view-only album; the tooltip says why.
  const tidyOff = status === null ? undefined : (viewOnlyReason(status) ?? undefined);
  const banner = status === null ? null : viewOnlyBanner(status);
  const scoped = status !== null && status.scope !== "";

  return (
    <header className="ow-topbar" data-region="top">
      <div className="ow-topbar__left">
        <span className="ow-topbar__name">{appText.name}</span>
        {review && <QueryBox />}
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
          <SaveAllButton viewOnly={tidyOff} />
          <ThumbSize />
          {narrow ? (
            <>
              <IconButton
                icon="tidy"
                label={albumText.tidyTooltip}
                disabledReason={tidyOff}
                onClick={() => setTidying(true)}
              />
              <IconButton icon="close-album" label={albumText.closeFolder} onClick={() => void close()} />
              <IconButton icon="folder" label={albumText.openFolder} onClick={() => void pick()} />
            </>
          ) : (
            <>
              {tidyOff !== undefined ? (
                <Button icon="tidy" disabledReason={tidyOff}>
                  {albumText.tidy}
                </Button>
              ) : (
                <Tooltip text={albumText.tidyTooltip}>
                  <Button icon="tidy" onClick={() => setTidying(true)}>
                    {albumText.tidy}
                  </Button>
                </Tooltip>
              )}
              <Button icon="close-album" onClick={() => void close()}>
                {albumText.closeFolder}
              </Button>
              <Button icon="folder" onClick={() => void pick()}>
                {albumText.openFolder}
              </Button>
            </>
          )}
          <TidyDialog open={tidying} onClose={() => setTidying(false)} />
        </div>
      )}
      <div className="ow-topbar__settings">
        <IconButton icon="settings" label={settingsText.open} onClick={() => openSettings()} />
      </div>
    </header>
  );
}
