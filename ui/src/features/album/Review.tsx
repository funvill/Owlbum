import { useCallback, useEffect, useState } from "react";
import type { Point } from "../../controls/place";
import type { PhotoAt } from "../../api/bindings/PhotoAt";
import { albumText } from "../../text/album";
import { DetailsPanel, useDetail, type CurrentPhoto } from "../details";
import { PhotoMenu, pressPhoto, useFileKeys } from "../files";
import { useRotateKeys } from "../edit";
import { BrowseGrid, gridBlocks, selectNone, useGridStore } from "../grid";
import { MapView } from "../map";
import { toggleStack, toggleTileStar, useFlagKeys } from "../organise";
import { Preview } from "../preview";
import { ViewBar } from "../timeline";
import { Viewer, openViewer, useViewerStore } from "../viewer";
import { viewOnlyReason } from "./banners";
import { useAlbumStore } from "./session";

/**
 * The review screen (docs/04-browsing.md, "Review screen"): the thumbnail
 * column, the preview and the details column. The splitters and the stored
 * column widths come later (#111).
 *
 * The preview and the details panel show the page's one current photo,
 * which the grid store holds and every `query_run` settles, so they can
 * never disagree. The details are read once here for both: the panel shows
 * them, the preview takes the original's size (largest zoom, full-size
 * swap) and full path from them. The row's file name, when the grid holds
 * the row, is shown in the panel's head while the details load.
 *
 * A double-click on a tile or on the preview at fit, or Enter or Space,
 * opens the full-screen viewer, which shows the same current photo and
 * takes the same details.
 */
export function Review({ busy, onOpen }: { busy: () => boolean; onOpen?: (at: PhotoAt) => void }) {
  const current = useGridStore((s) => s.current);
  const row = current === null ? undefined : gridBlocks().row(current.index);
  const shown: CurrentPhoto | null =
    current === null
      ? null
      : {
          id: current.id,
          // Only a row that is this photo's: the index may be an old result's.
          file_name: row?.id === current.id ? row.file_name : null,
          rel_path: row?.id === current.id ? row.rel_path : null,
        };
  const notFound = useCallback(() => void useGridStore.getState().refresh(), []);
  const detail = useDetail(current?.id ?? null, notFound);
  const open = onOpen ?? openViewer;
  // F, S and H star and hide (docs/05-organising.md, "Star and hide"); in a
  // view-only album the tile's star is a badge, not a button.
  useFlagKeys();
  // Delete and Ctrl+C (docs/06-editing-and-file-actions.md, "File actions").
  useFileKeys();
  // The photo menu: a right-click on a tile or the preview.
  const [menu, setMenu] = useState<{ at: Point; photoId: number } | null>(null);
  const closeMenu = useCallback(() => setMenu(null), []);
  const openMenu = useCallback((at: PhotoAt, e: { clientX: number; clientY: number }) => {
    setMenu({ at: { x: e.clientX, y: e.clientY }, photoId: at.id });
  }, []);
  // [ and ] turn the selection (docs/06-editing-and-file-actions.md).
  useRotateKeys();
  const writable = useAlbumStore((s) => s.status === null || viewOnlyReason(s.status) === null);
  const folder = useAlbumStore((s) => s.status?.folder ?? "");
  const view = useGridStore((s) => s.query.view);
  // The viewer belongs to the open album: it does not come back with the next one.
  useEffect(() => () => useViewerStore.setState({ open: false }), []);

  return (
    <section className="ow-review" aria-label={albumText.photosRegion}>
      <div className="ow-review__photos">
        <ViewBar />
        {view === "map" ? (
          <MapView album={folder} onOpen={open} />
        ) : (
          <BrowseGrid
            busy={busy}
            onOpen={open}
            onStar={writable ? (r) => void toggleTileStar(r) : undefined}
            onStack={toggleStack}
            onMenu={openMenu}
            onPress={(at, e) => pressPhoto(at.id, e)}
          />
        )}
      </div>
      <div className="ow-review__preview">
        <Preview detail={detail.detail} onOpen={open} onMenu={openMenu} />
      </div>
      <div className="ow-review__details">
        {/* Close details does what Ctrl+D does: nothing selected. */}
        <DetailsPanel current={shown} onClose={selectNone} onNotFound={notFound} shared={detail} />
      </div>
      <Viewer detail={detail.detail} />
      {menu !== null && <PhotoMenu at={menu.at} photoId={menu.photoId} onClose={closeMenu} />}
    </section>
  );
}
