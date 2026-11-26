import { useCallback } from "react";
import { albumText } from "../../text/album";
import { DetailsPanel, type CurrentPhoto } from "../details";
import { BrowseGrid, gridBlocks, selectNone, useGridStore } from "../grid";

/**
 * The review screen (docs/04-browsing.md, "Review screen"): the thumbnail
 * column and the details column. The preview (#20) and the splitters come
 * later.
 *
 * The details panel shows the page's one current photo, which the grid
 * store holds and every `query_run` settles, so the preview and the
 * details can never disagree. The row's file name, when the grid holds the
 * row, is shown in the panel's head while the details load.
 */
export function Review({ busy }: { busy: () => boolean }) {
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
  // Close details does what Ctrl+D does: nothing selected.
  const notFound = useCallback(() => void useGridStore.getState().refresh(), []);

  return (
    <section className="ow-review" aria-label={albumText.photosRegion}>
      <div className="ow-review__photos">
        <BrowseGrid busy={busy} />
      </div>
      <div className="ow-review__details">
        <DetailsPanel current={shown} onClose={selectNone} onNotFound={notFound} />
      </div>
    </section>
  );
}
