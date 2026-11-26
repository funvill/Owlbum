import { useCallback, useEffect } from "react";
import type { PhotoAt } from "../../api/bindings/PhotoAt";
import { albumText } from "../../text/album";
import { DetailsPanel, useDetail, type CurrentPhoto } from "../details";
import { BrowseGrid, gridBlocks, selectNone, useGridStore } from "../grid";
import { toggleTileStar, useFlagKeys } from "../organise";
import { Preview } from "../preview";
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
  const writable = useAlbumStore((s) => s.status === null || viewOnlyReason(s.status) === null);
  // The viewer belongs to the open album: it does not come back with the next one.
  useEffect(() => () => useViewerStore.setState({ open: false }), []);

  return (
    <section className="ow-review" aria-label={albumText.photosRegion}>
      <div className="ow-review__photos">
        <BrowseGrid busy={busy} onOpen={open} onStar={writable ? (r) => void toggleTileStar(r) : undefined} />
      </div>
      <div className="ow-review__preview">
        <Preview detail={detail.detail} onOpen={open} />
      </div>
      <div className="ow-review__details">
        {/* Close details does what Ctrl+D does: nothing selected. */}
        <DetailsPanel current={shown} onClose={selectNone} onNotFound={notFound} shared={detail} />
      </div>
      <Viewer detail={detail.detail} />
    </section>
  );
}
