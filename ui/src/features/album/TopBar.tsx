import { useState } from "react";
import { SaveAllButton } from "../edit/SaveAllButton";
import { Button, IconButton, Pill, Tooltip, useNarrowWindow } from "../../controls";
import { albumText } from "../../text/album";
import { appText, settingsText } from "../../text/app";
import { openSettings } from "../app/settingsOpen";
import { ThumbSize } from "../grid";
import { shownScope, viewOnlyBanner, viewOnlyReason } from "./banners";
import { TidyDialog } from "./TidyDialog";
import { useAlbumStore } from "./session";

/**
 * The 48 px bar at the top of the window (docs/04-browsing.md, "Top bar").
 * The gear that opens Settings is always there.
 */
export function TopBar() {
  const status = useAlbumStore((s) => s.status);
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
  const tooltip =
    status === null
      ? ""
      : scoped
        ? `${albumText.albumTooltip(status.root)}\n${albumText.scopeTooltip(status.folder)}`
        : albumText.albumTooltip(status.root);

  return (
    <header className="ow-topbar">
      <div className="ow-topbar__left">
        <span className="ow-topbar__name">{appText.name}</span>
        {status !== null && (
          <Tooltip text={tooltip} describe={false}>
            <span className="ow-topbar__path ow-selectable" tabIndex={0} aria-label={tooltip}>
              <span className="ow-topbar__root">
                <bdi>{status.root}</bdi>
              </span>
              {scoped && (
                <>
                  <span className="ow-topbar__sep" aria-hidden="true">
                    ›
                  </span>
                  <span className="ow-topbar__scope">{shownScope(status)}</span>
                </>
              )}
            </span>
          </Tooltip>
        )}
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
        <IconButton icon="settings" label={settingsText.open} onClick={openSettings} />
      </div>
    </header>
  );
}
