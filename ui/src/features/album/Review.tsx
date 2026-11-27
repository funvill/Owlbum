import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import type { Point } from "../../controls/place";
import type { PhotoAt } from "../../api/bindings/PhotoAt";
import { albumText } from "../../text/album";
import { DetailsPanel, useDetail, type CurrentPhoto } from "../details";
import { PhotoMenu, pressPhoto, useFileKeys } from "../files";
import { useReviewEdit, useRotateKeys } from "../edit";
import { BrowseGrid, useGridStore } from "../grid";
import { MapView } from "../map";
import { toggleStack, toggleTileStar, useFlagKeys } from "../organise";
import { Preview } from "../preview";
import { ViewBar } from "../timeline";
import { Viewer, openViewer, useViewerStore } from "../viewer";
import { viewOnlyReason } from "./banners";
import { useAlbumStore } from "./session";
import { useSettingsStore } from "../app";
import { DETAILS_DEFAULT, DETAILS_MIN, GRID_DEFAULT, GRID_MIN, columnWidths } from "./columns";
import { Splitter } from "./Splitter";

type Width = "panel.grid_width" | "panel.details_width";

/**
 * The review row's column widths: the stored ones (settings), a width being
 * moved by a splitter until it is saved, and the row's measured width.
 */
function useColumns() {
  const ref = useRef<HTMLElement>(null);
  const [room, setRoom] = useState<number | null>(null);
  const stored = useSettingsStore((s) => s.settings);
  const [moving, setMoving] = useState<Partial<Record<Width, number>>>({});

  useLayoutEffect(() => {
    const el = ref.current;
    if (el === null) return;
    // A row not laid out yet (width 0) counts as not measured.
    const measure = () => setRoom(el.clientWidth > 0 ? el.clientWidth : null);
    measure();
    if (typeof ResizeObserver === "undefined") {
      window.addEventListener("resize", measure);
      return () => window.removeEventListener("resize", measure);
    }
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  const grid = moving["panel.grid_width"] ?? stored?.["panel.grid_width"] ?? GRID_DEFAULT;
  const details = moving["panel.details_width"] ?? stored?.["panel.details_width"] ?? DETAILS_DEFAULT;
  const move = useCallback((key: Width, width: number) => setMoving((m) => ({ ...m, [key]: width })), []);
  const save = useCallback(async (key: Width, width: number) => {
    setMoving((m) => ({ ...m, [key]: width }));
    await useSettingsStore.getState().update({ [key]: width });
    // The store holds Rust's answer now (or the old value, if it was refused).
    setMoving((m) => {
      if (m[key] !== width) return m;
      const rest = { ...m };
      delete rest[key];
      return rest;
    });
  }, []);
  return { ref, columns: columnWidths(room, grid, details), move, save };
}

/**
 * The review screen (docs/04-browsing.md, "Review screen"): the thumbnail
 * column, the preview and the details column, with a splitter between each
 * two. The column widths are the stored settings `panel.grid_width` and
 * `panel.details_width`, narrowed to fit the row without changing them.
 *
 * The preview and the details panel show the page's one current photo,
 * which the grid store holds and every `query_run` settles, so they can
 * never disagree. The details are read once here for both: the panel shows
 * them, the preview takes the original's size (largest zoom, full-size
 * swap) and full path from them.
 *
 * A double-click on a tile or on the preview at fit, or Enter or Space,
 * opens the full-screen viewer, which shows the same current photo and
 * takes the same details.
 */
export function Review({ busy, onOpen }: { busy: () => boolean; onOpen?: (at: PhotoAt) => void }) {
  const current = useGridStore((s) => s.current);
  const shown: CurrentPhoto | null = current === null ? null : { id: current.id };
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
  // E opens edit mode, \ holds Before; leaving the album ends edit mode as Done.
  useReviewEdit();
  const writable = useAlbumStore((s) => s.status === null || viewOnlyReason(s.status) === null);
  const folder = useAlbumStore((s) => s.status?.folder ?? "");
  const view = useGridStore((s) => s.query.view);
  // The viewer belongs to the open album: it does not come back with the next one.
  useEffect(() => () => useViewerStore.setState({ open: false }), []);
  const { ref, columns, move, save } = useColumns();

  return (
    <section ref={ref} className="ow-review" aria-label={albumText.photosRegion}>
      <div className="ow-review__photos" style={{ width: columns.grid }}>
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
      <Splitter
        label={albumText.resizeGrid}
        value={columns.grid}
        min={GRID_MIN}
        max={columns.gridMax}
        dir={1}
        reset={GRID_DEFAULT}
        onMove={(w) => move("panel.grid_width", w)}
        onSave={(w) => void save("panel.grid_width", w)}
      />
      <div className="ow-review__preview">
        <Preview detail={detail.detail} onOpen={open} onMenu={openMenu} />
      </div>
      <Splitter
        label={albumText.resizeDetails}
        value={columns.details}
        min={DETAILS_MIN}
        max={columns.detailsMax}
        dir={-1}
        reset={DETAILS_DEFAULT}
        onMove={(w) => move("panel.details_width", w)}
        onSave={(w) => void save("panel.details_width", w)}
      />
      <div className="ow-review__details" style={{ width: columns.details }}>
        <DetailsPanel current={shown} onNotFound={notFound} shared={detail} />
      </div>
      <Viewer detail={detail.detail} />
      {menu !== null && <PhotoMenu at={menu.at} photoId={menu.photoId} onClose={closeMenu} />}
    </section>
  );
}
