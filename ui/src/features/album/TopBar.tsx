import { useSyncExternalStore } from "react";
import { Button, IconButton, Pill, Tooltip } from "../../controls";
import { albumText } from "../../text/album";
import { appText } from "../../text/app";
import { shownScope, viewOnlyBanner } from "./banners";
import { useAlbumStore } from "./session";

/** Below this width the bar's buttons become icon buttons (docs/09-visual-design.md, "The bars"). */
const NARROW = "(max-width: 1099px)";

function narrowQuery(): MediaQueryList | null {
  return typeof window.matchMedia === "function" ? window.matchMedia(NARROW) : null;
}

function subscribeNarrow(changed: () => void): () => void {
  const query = narrowQuery();
  query?.addEventListener("change", changed);
  return () => query?.removeEventListener("change", changed);
}

/** True when the window is narrower than 1100 px. */
function useNarrow(): boolean {
  return useSyncExternalStore(subscribeNarrow, () => narrowQuery()?.matches ?? false);
}

/**
 * The 48 px bar at the top of the window (docs/04-browsing.md, "Top bar").
 * The thumbnail size control comes with the grid (#18), Tidy… with #34
 * and Settings with #59.
 */
export function TopBar() {
  const status = useAlbumStore((s) => s.status);
  const openFolder = useAlbumStore((s) => s.openFolder);
  const pick = useAlbumStore((s) => s.pickFolder);
  const close = useAlbumStore((s) => s.close);
  const narrow = useNarrow();
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
    </header>
  );
}
