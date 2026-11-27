import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
  type KeyboardEvent,
} from "react";
import { gridText } from "../../text/grid";
import { timelineText } from "../../text/timeline";
import { useSettingsStore } from "../app";
import {
  PAD,
  anchorAfterResize,
  followScroll,
  gridLayout,
  realScroll,
  scrollRatio,
  scrollToShow,
  thumbFor,
  toScrollTop,
  viewIndex,
  visibleRange,
  type GridLayout,
  type VisibleRange,
} from "./layout";
import { HEAD, groupOfRow, rowAt, rowTiles, rowTop } from "./groups";
import { selectGroup } from "./selection";
import {
  animatedLabel,
  gridBlocks,
  setGridTop,
  setGridView,
  useGridStore,
} from "./store";
import { Tile, type TileActions } from "./Tile";
import "./Grid.css";

export interface GridProps extends TileActions {
  /** Whether a scan or tagging is running, which spaces re-runs a second apart. */
  busy?: () => boolean;
}

/** Pixels per wheel line, for a wheel that reports lines. */
const LINE = 16;

const sameRange = (a: VisibleRange, b: VisibleRange) =>
  a.firstRow === b.firstRow &&
  a.endRow === b.endRow &&
  a.lastIndex === b.lastIndex;

/** The timeline's sticky heading: the group at the top of the view, pushed up by the next heading. */
interface Sticky {
  group: number;
  y: number;
}

/** The sticky heading for real scroll `real`, or null where none shows (the grid, or a heading in its own place). */
function stickyAt(l: GridLayout, real: number): Sticky | null {
  if (l.groups === null || l.rows === 0) return null;
  const top = real - PAD;
  const group = groupOfRow(l, rowAt(l, Math.max(0, top)));
  const own = rowTop(l, l.groups.headRows[group]!);
  if (top <= own) return null;
  const next = l.groups.headRows[group + 1];
  const y = next === undefined ? 0 : Math.min(0, rowTop(l, next) - top - HEAD);
  return { group, y };
}

const sameSticky = (a: Sticky | null, b: Sticky | null) =>
  a === b || (a !== null && b !== null && a.group === b.group && a.y === b.y);

/**
 * The grid of the current result (docs/04-browsing.md, "The grid"): a
 * scroll box in which only the rows in view, plus the overscan, exist.
 * It starts the grid store when it first mounts; the album's lifetime
 * (`useGridStore.getState().stop()` on close) belongs to the window shell.
 */
export function Grid(props: GridProps) {
  const result = useGridStore((s) => s.result);
  const thumbSize = useGridStore((s) => s.thumbSize);
  const current = useGridStore((s) => s.current);
  const anchor = useGridStore((s) => s.anchor);
  const selection = useGridStore((s) => s.selection);
  const pending = useGridStore((s) => s.pending);
  const scroll = useGridStore((s) => s.scroll);
  const follow = useGridStore((s) => s.follow);
  useGridStore((s) => s.formats);
  const editTarget = useSettingsStore(
    (s) => s.settings?.edit_target ?? "album",
  );
  const count = result?.count ?? 0;
  const groups = result?.groups ?? null;

  const blocks = useMemo(() => gridBlocks(), []);
  useSyncExternalStore(
    useCallback((listener: () => void) => blocks.subscribe(listener), [blocks]),
    () => blocks.version,
  );

  const busy = useRef(props.busy);
  useEffect(() => {
    void useGridStore
      .getState()
      .start({ busy: () => busy.current?.() ?? false });
  }, []);
  useEffect(() => {
    busy.current = props.busy;
  });

  const boxRef = useRef<HTMLDivElement>(null);
  const layerRef = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ width: 0, height: 0 });
  const layout = useMemo(
    () => gridLayout(size.width, count, thumbSize, groups),
    [size.width, count, thumbSize, groups],
  );
  const [range, setRange] = useState<VisibleRange>(() =>
    visibleRange(layout, 0, 0, count),
  );
  const [sticky, setSticky] = useState<Sticky | null>(null);

  // The latest values, for the handlers attached once.
  // `at` is where the current photo is, or is going while a step waits on its row.
  const at = pending?.index ?? current?.index ?? null;
  const live = useRef({
    layout,
    size,
    count,
    at,
    anchor,
    real: 0,
    direction: 1 as 1 | -1,
    frame: 0,
  });
  // Kept up to date before any other effect of this render reads it.
  useLayoutEffect(() => {
    Object.assign(live.current, { layout, size, count, at, anchor });
  });

  /** The first row drawn, as committed: rows are placed relative to it. */
  const drawnFirst = useRef(0);

  /**
   * Moves the layer of rows so the drawn rows sit where they belong in the
   * box. Rows are placed from the first drawn row, so no transform is ever
   * near the real height of a very tall result.
   */
  const placeLayer = useCallback(() => {
    const el = boxRef.current;
    const layer = layerRef.current;
    if (el === null || layer === null) return;
    const v = live.current;
    const y =
      el.scrollTop - v.real + PAD + rowTop(v.layout, drawnFirst.current);
    layer.style.transform = `translateY(${y}px)`;
  }, []);

  /** Reads the scroll position and works out what is drawn and wanted. */
  const update = useCallback(() => {
    const el = boxRef.current;
    if (el === null) return;
    const v = live.current;
    const viewport = v.size.height;
    const real = realScroll(el.scrollTop, v.layout, viewport);
    if (real !== v.real) v.direction = real > v.real ? 1 : -1;
    v.real = real;
    placeLayer();
    const next = visibleRange(v.layout, real, viewport, v.count);
    setRange((before) => (sameRange(before, next) ? before : next));
    // The first photo in view, kept with the album's session.
    setGridTop(viewIndex(v.layout, real, viewport, v.count, null) ?? 0);
    const nextSticky = stickyAt(v.layout, real);
    setSticky((before) =>
      sameSticky(before, nextSticky) ? before : nextSticky,
    );
    blocks.want({
      first: next.firstIndex,
      last: next.lastIndex,
      span: Math.max(1, Math.ceil(viewport / v.layout.stride) * v.layout.cols),
      direction: v.direction,
      current: v.at,
      anchor: v.anchor?.index ?? null,
    });
  }, [blocks, placeLayer]);

  // The tile the place is kept by while rows arrive, worked out when a re-run
  // asks: only a row of the result now held counts.
  useEffect(() => {
    setGridView(() => {
      const v = live.current;
      const index = viewIndex(v.layout, v.real, v.size.height, v.count, v.at);
      if (index === null || !blocks.fresh(index)) return null;
      const row = blocks.row(index);
      return row === undefined ? null : { id: row.id, index };
    });
    return () => setGridView(() => null);
  }, [blocks]);

  /** Sets the real scroll position. */
  const scrollReal = useCallback((real: number) => {
    const el = boxRef.current;
    if (el === null) return;
    const v = live.current;
    el.scrollTop = toScrollTop(Math.max(0, real), v.layout, v.size.height);
  }, []);

  // The box's size.
  useLayoutEffect(() => {
    const el = boxRef.current;
    if (el === null) return;
    const measure = () =>
      setSize((s) =>
        s.width === el.clientWidth && s.height === el.clientHeight
          ? s
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

  // Above the height limit the page scrolls by the wheel and page keys itself, in real pixels.
  useEffect(() => {
    const el = boxRef.current;
    if (el === null) return;
    const onWheel = (e: WheelEvent) => {
      const v = live.current;
      const ratio = scrollRatio(v.layout, v.size.height);
      if (ratio === 1 || e.ctrlKey) return;
      e.preventDefault();
      const unit =
        e.deltaMode === 1 ? LINE : e.deltaMode === 2 ? v.size.height : 1;
      el.scrollTop += (e.deltaY * unit) / ratio;
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, []);

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.key !== "PageDown" && e.key !== "PageUp") return;
    if (e.ctrlKey || e.altKey || e.metaKey || e.shiftKey) return;
    const v = live.current;
    if (scrollRatio(v.layout, v.size.height) === 1) return;
    e.preventDefault();
    scrollReal(v.real + (e.key === "PageDown" ? 1 : -1) * v.size.height);
  };

  // A new layout: keep the top-left photo in the top row, keep the current photo in view.
  const before = useRef<GridLayout | null>(null);
  useLayoutEffect(() => {
    const old = before.current;
    before.current = layout;
    const v = live.current;
    if (
      old !== null &&
      old.width > 0 &&
      layout.width > 0 &&
      (old.cols !== layout.cols || old.stride !== layout.stride)
    ) {
      scrollReal(anchorAfterResize(old, layout, v.real));
      v.real = realScroll(
        boxRef.current?.scrollTop ?? 0,
        layout,
        v.size.height,
      );
      if (old.cols !== layout.cols && v.at !== null) {
        const to = scrollToShow(v.at, layout, v.real, v.size.height);
        if (to !== null) scrollReal(to);
      }
    } else {
      // The count or the viewport changed. Above MAX_SCROLL that changes the
      // scale, so the box's scrollTop is set again for the same real place:
      // a scan adding photos does not move the view.
      scrollReal(v.real);
    }
    update();
  }, [layout, size.height, update, scrollReal]);

  // Rows arrived before the tile the place is kept by: scroll by as many rows,
  // so the photos in view stay where they were (docs/04, "When the result changes").
  useLayoutEffect(() => {
    if (follow === null) return;
    const v = live.current;
    scrollReal(followScroll(v.layout, v.real, follow.from, follow.to));
    update();
  }, [follow, scrollReal, update]);

  // Scrolling asked for by the store: to the current photo, or to the top.
  useLayoutEffect(() => {
    if (scroll === null) return;
    const v = live.current;
    if (scroll.to === "top") scrollReal(0);
    else if (v.at !== null) {
      const to = scrollToShow(v.at, v.layout, v.real, v.size.height);
      if (to !== null) scrollReal(to);
    }
    update();
  }, [scroll, scrollReal, update]);

  // The current photo or the anchor moved: their blocks are wanted and kept.
  useEffect(() => {
    update();
  }, [at, anchor, update]);

  // Keyboard focus inside the grid follows the current photo.
  useEffect(() => {
    const el = boxRef.current;
    if (el === null || at === null || !el.contains(document.activeElement))
      return;
    const tile = el.querySelector<HTMLElement>(`[data-index="${at}"]`);
    if (tile !== null && tile !== document.activeElement)
      tile.focus({ preventScroll: true });
  }, [at, range]);

  // New rows are drawn: place the layer for them.
  useLayoutEffect(() => {
    drawnFirst.current = range.firstRow;
    placeLayer();
  }, [range, layout, placeLayer]);

  const onScroll = () => {
    const v = live.current;
    if (v.frame !== 0) return;
    v.frame = requestAnimationFrame(() => {
      v.frame = 0;
      update();
    });
  };
  useEffect(() => () => cancelAnimationFrame(live.current.frame), []);

  const selected = useMemo(() => new Set(selection), [selection]);
  const actions = useRef<TileActions>(props);
  useLayoutEffect(() => {
    actions.current = props;
  });
  // One stable object, so tiles redraw only when their own props change.
  const hasMenu = props.onMenu !== undefined;
  const hasStar = props.onStar !== undefined;
  const hasStack = props.onStack !== undefined;
  const stableActions = useMemo<TileActions>(
    () => ({
      onTileClick: (at, e) => actions.current.onTileClick?.(at, e),
      onOpen: (at) => actions.current.onOpen?.(at),
      onMenu: hasMenu ? (at, e) => actions.current.onMenu?.(at, e) : undefined,
      onStar: hasStar ? (row) => actions.current.onStar?.(row) : undefined,
      onStack: hasStack ? (row) => actions.current.onStack?.(row) : undefined,
    }),
    [hasMenu, hasStar, hasStack],
  );

  if (result !== null && count === 0 && result.album_photos > 0) {
    const s = useGridStore.getState();
    return (
      <div className="ow-grid ow-grid--empty">
        <p className="ow-grid__message">{gridText.noMatch}</p>
        <button
          type="button"
          className="ow-grid__link"
          onClick={() => s.setQuery({ ...s.query, filter: {}, search: null })}
        >
          {gridText.clearFilter}
        </button>
      </div>
    );
  }

  const thumb = thumbFor(layout.side, globalThis.devicePixelRatio || 1);
  const focusIndex =
    at !== null && at >= range.firstIndex && at <= range.lastIndex
      ? at
      : range.firstIndex;
  // While a change waits on the backend, its range shows selected by index.
  const shownByIndex = (i: number) =>
    pending !== null && i >= pending.first && i <= pending.last;
  const others = pending === null || pending.add;
  const rows = [];
  const firstTop = rowTop(layout, range.firstRow);
  const heading = (g: number, copy = false) => {
    const group = groups?.[g];
    if (group === undefined) return null;
    return (
      <button
        type="button"
        className="ow-grid__head"
        tabIndex={copy ? -1 : undefined}
        aria-label={timelineText.selectGroup(group.key, group.photos)}
        onClick={(e) =>
          void selectGroup(group.start, group.count, e.ctrlKey || e.metaKey)
        }
      >
        {timelineText.heading(group.key, group.photos)}
      </button>
    );
  };
  for (let r = range.firstRow; r < range.endRow; r++) {
    const tiles: [number, number] | null =
      layout.groups === null
        ? [r * layout.cols, Math.min(count, (r + 1) * layout.cols)]
        : rowTiles(layout, r);
    if (tiles === null) {
      // A timeline heading: a click selects its group.
      rows.push(
        <div
          key={r}
          role="row"
          aria-rowindex={r + 1}
          className="ow-grid__row ow-grid__head-row"
          style={{
            height: HEAD,
            transform: `translateY(${rowTop(layout, r) - firstTop}px)`,
          }}
        >
          <div role="rowheader" className="ow-grid__head-cell">
            {heading(groupOfRow(layout, r))}
          </div>
        </div>,
      );
      continue;
    }
    const [first, end] = tiles;
    const cells = [];
    for (let i = first; i < end; i++) {
      const row = blocks.row(i);
      cells.push(
        <Tile
          key={i}
          index={i}
          row={row}
          x={PAD + (i - first) * layout.stride}
          side={layout.side}
          thumb={thumb}
          selected={
            shownByIndex(i) ||
            (others && row !== undefined && selected.has(row.id))
          }
          current={at === i}
          tabbable={i === focusIndex}
          editTarget={editTarget}
          animatedLabel={
            row === undefined ? null : animatedLabel(row.file_name)
          }
          actions={stableActions}
        />,
      );
    }
    rows.push(
      <div
        key={r}
        role="row"
        aria-rowindex={r + 1}
        className="ow-grid__row"
        style={{
          height: layout.side,
          transform: `translateY(${rowTop(layout, r) - firstTop}px)`,
        }}
      >
        {cells}
      </div>,
    );
  }

  return (
    <div
      ref={boxRef}
      className="ow-grid"
      role="grid"
      aria-label={gridText.label}
      aria-multiselectable="true"
      aria-rowcount={layout.rows}
      aria-colcount={layout.cols}
      aria-busy={result === null}
      onScroll={onScroll}
      onKeyDown={onKeyDown}
    >
      {sticky !== null && (
        // A copy of the heading in its own place, which the screen reader already has.
        <div className="ow-grid__sticky" aria-hidden="true">
          <div
            className="ow-grid__sticky-head"
            style={{ transform: `translateY(${sticky.y}px)` }}
          >
            {heading(sticky.group, true)}
          </div>
        </div>
      )}
      <div
        className="ow-grid__content"
        style={{ height: `${layout.height}px` }}
      >
        <div ref={layerRef} className="ow-grid__layer">
          {rows}
        </div>
      </div>
    </div>
  );
}
