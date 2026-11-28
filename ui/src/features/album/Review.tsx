import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import type { Point } from "../../controls/place";
import type { PhotoAt } from "../../api/bindings/PhotoAt";
import { albumText } from "../../text/album";
import { DetailsPanel, useDetail, type CurrentPhoto } from "../details";
import { PhotoMenu, pressPhoto, useFileKeys } from "../files";
import { useReviewEdit, useRotateKeys } from "../edit";
import { BrowseGrid, selectById, useGridStore } from "../grid";
import { MapView } from "../map";
import { toggleStack, toggleTileStar, useFlagKeys } from "../organise";
import { Preview } from "../preview";
import { ViewBar } from "../timeline";
import { Viewer, openViewer, useViewerStore } from "../viewer";
import { viewOnlyReason } from "./banners";
import { useAlbumStore } from "./session";
import { useSettingsStore } from "../app";
import { IconButton } from "../../controls";
import { primaryKey } from "../../keys";
import {
  DETAILS_DEFAULT,
  DETAILS_MIN,
  GRID_DEFAULT,
  GRID_MIN,
  STRIP_DEFAULT,
  STRIP_MIN,
  columnWidths,
  stripHeight,
  type InRow,
} from "./columns";
import { toggleDetails, toggleGrid, usePanelKeys, usePanels, type Panels } from "./panels";
import { Splitter } from "./Splitter";

type Width = "panel.grid_width" | "panel.details_width" | "panel.strip_height";

/**
 * The review row's column widths: the stored ones (settings), a width being
 * moved by a splitter until it is saved, and the row's measured width.
 */
function useColumns(inRow: InRow) {
  const ref = useRef<HTMLElement>(null);
  const [room, setRoom] = useState<{ width: number; height: number } | null>(null);
  const stored = useSettingsStore((s) => s.settings);
  const [moving, setMoving] = useState<Partial<Record<Width, number>>>({});

  useLayoutEffect(() => {
    const el = ref.current;
    if (el === null) return;
    // A row not laid out yet (width 0) counts as not measured.
    const measure = () =>
      setRoom((r) =>
        el.clientWidth <= 0
          ? null
          : r !== null && r.width === el.clientWidth && r.height === el.clientHeight
            ? r
            : { width: el.clientWidth, height: el.clientHeight },
      );
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
  const strip = moving["panel.strip_height"] ?? stored?.["panel.strip_height"] ?? STRIP_DEFAULT;
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
  return {
    ref,
    columns: columnWidths(room?.width ?? null, grid, details, inRow),
    strip: stripHeight(room?.height ?? null, strip),
    move,
    save,
  };
}

/**
 * The panel buttons at the preview's top right (docs/04-browsing.md,
 * "Hiding and moving the panels"): each shows or hides its panel, and is
 * pressed while the panel shows.
 */
function PanelButtons({ panels }: { panels: Panels }) {
  return (
    <div className="ow-review__panels" role="toolbar" aria-label={albumText.panels}>
      <IconButton
        icon={panels.place === "bottom" ? "panel-bottom" : "panel-left"}
        label={albumText.thumbnails}
        shortcut={primaryKey("browse.toggle_grid")}
        pressed={panels.grid}
        onClick={() => void toggleGrid()}
      />
      <IconButton
        icon="details"
        label={albumText.details}
        shortcut={primaryKey("browse.toggle_details")}
        pressed={panels.details}
        onClick={() => void toggleDetails()}
      />
    </div>
  );
}

/**
 * The review screen (docs/04-browsing.md, "Review screen"): the thumbnail
 * column, the preview and the details column, with a splitter between each
 * two. The column widths are the stored settings `panel.grid_width` and
 * `panel.details_width`, narrowed to fit the row without changing them.
 * The thumbnails and the details can be hidden, and the thumbnails can sit
 * in a strip along the bottom instead (`panel.grid_shown`,
 * `panel.details_shown`, `panel.grid_place`, `panel.strip_height`). Hidden
 * thumbnails stay mounted, so their keys, the rows they ask for and the
 * selection go on working; a hidden details panel is not drawn.
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
  // The photo menu: a right-click on a tile, the preview, the viewer or a map marker.
  const [menu, setMenu] = useState<{ at: Point; photoId: number } | null>(null);
  const closeMenu = useCallback(() => setMenu(null), []);
  const openMenu = useCallback((at: PhotoAt, e: { clientX: number; clientY: number }) => {
    setMenu({ at: { x: e.clientX, y: e.clientY }, photoId: at.id });
  }, []);
  // A marker's photo becomes the selection first unless it is in it already.
  const openMarkerMenu = useCallback((id: number, at: Point) => {
    void (async () => {
      if (!useGridStore.getState().selection.includes(id)) await selectById(id);
      setMenu({ at, photoId: id });
    })();
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
  // T and I hide or show the thumbnails and the details panel.
  usePanelKeys();
  const panels = usePanels();
  const bottom = panels.place === "bottom";
  const { ref, columns, strip, move, save } = useColumns({ grid: panels.grid && !bottom, details: panels.details });

  const photos = (
    <div
      className={bottom ? "ow-review__photos ow-review__photos--strip" : "ow-review__photos"}
      style={bottom ? { height: strip.height } : { width: columns.grid }}
      data-region="grid"
      hidden={!panels.grid}
    >
      <ViewBar />
      {view === "map" ? (
        <MapView album={folder} onOpen={open} onMenu={openMarkerMenu} />
      ) : (
        <BrowseGrid
          busy={busy}
          across={bottom}
          onOpen={open}
          onStar={writable ? (r) => void toggleTileStar(r) : undefined}
          onStack={toggleStack}
          onMenu={openMenu}
          onPress={(at, e) => pressPhoto(at.id, e)}
        />
      )}
    </div>
  );
  const row = (
    <>
      {!bottom && photos}
      {!bottom && panels.grid && (
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
      )}
      <div className="ow-review__preview" data-region="preview">
        <Preview
          detail={detail.detail}
          onOpen={open}
          onMenu={openMenu}
          onPress={(at, e) => pressPhoto(at.id, e)}
        />
        <PanelButtons panels={panels} />
      </div>
      {panels.details && (
        <>
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
          <div className="ow-review__details" style={{ width: columns.details }} data-region="details">
            <DetailsPanel current={shown} onNotFound={notFound} shared={detail} />
          </div>
        </>
      )}
    </>
  );

  return (
    <section
      ref={ref}
      className={bottom ? "ow-review ow-review--bottom" : "ow-review"}
      aria-label={albumText.photosRegion}
    >
      {bottom ? (
        <>
          <div className="ow-review__row">{row}</div>
          {panels.grid && (
            <Splitter
              across
              label={albumText.resizeStrip}
              value={strip.height}
              min={STRIP_MIN}
              max={strip.max}
              dir={-1}
              reset={STRIP_DEFAULT}
              onMove={(h) => move("panel.strip_height", h)}
              onSave={(h) => void save("panel.strip_height", h)}
            />
          )}
          {photos}
        </>
      ) : (
        row
      )}
      <Viewer detail={detail.detail} onMenu={openMenu} />
      {menu !== null && <PhotoMenu at={menu.at} photoId={menu.photoId} onClose={closeMenu} />}
    </section>
  );
}
