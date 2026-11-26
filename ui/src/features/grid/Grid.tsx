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
import { useSettingsStore } from "../app";
import {
  PAD,
  anchorAfterResize,
  gridLayout,
  realScroll,
  scrollRatio,
  scrollToShow,
  thumbFor,
  toScrollTop,
  visibleRange,
  type GridLayout,
  type VisibleRange,
} from "./layout";
import { gridBlocks, useGridStore } from "./store";
import { Tile, type TileActions } from "./Tile";
import "./Grid.css";

export interface GridProps extends TileActions {
  /** Whether a scan or tagging is running, which spaces re-runs a second apart. */
  busy?: () => boolean;
}

/** Pixels per wheel line, for a wheel that reports lines. */
const LINE = 16;

const sameRange = (a: VisibleRange, b: VisibleRange) =>
  a.firstRow === b.firstRow && a.endRow === b.endRow && a.lastIndex === b.lastIndex;

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
  const scroll = useGridStore((s) => s.scroll);
  const editTarget = useSettingsStore((s) => s.settings?.edit_target ?? "album");
  const count = result?.count ?? 0;

  const blocks = useMemo(() => gridBlocks(), []);
  useSyncExternalStore(
    useCallback((listener: () => void) => blocks.subscribe(listener), [blocks]),
    () => blocks.version,
  );

  const busy = useRef(props.busy);
  useEffect(() => {
    void useGridStore.getState().start({ busy: () => busy.current?.() ?? false });
  }, []);
  useEffect(() => {
    busy.current = props.busy;
  });

  const boxRef = useRef<HTMLDivElement>(null);
  const layerRef = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ width: 0, height: 0 });
  const layout = useMemo(() => gridLayout(size.width, count, thumbSize), [size.width, count, thumbSize]);
  const [range, setRange] = useState<VisibleRange>(() => visibleRange(layout, 0, 0, count));

  // The latest values, for the handlers attached once.
  const live = useRef({ layout, size, count, current, anchor, real: 0, direction: 1 as 1 | -1, frame: 0 });
  // Kept up to date before any other effect of this render reads it.
  useLayoutEffect(() => {
    Object.assign(live.current, { layout, size, count, current, anchor });
  });

  /** Reads the scroll position and works out what is drawn and wanted. */
  const update = useCallback(() => {
    const el = boxRef.current;
    if (el === null) return;
    const v = live.current;
    const viewport = v.size.height;
    const real = realScroll(el.scrollTop, v.layout, viewport);
    if (real !== v.real) v.direction = real > v.real ? 1 : -1;
    v.real = real;
    if (layerRef.current !== null) layerRef.current.style.transform = `translateY(${el.scrollTop - real}px)`;
    const next = visibleRange(v.layout, real, viewport, v.count);
    setRange((before) => (sameRange(before, next) ? before : next));
    blocks.want({
      first: next.firstIndex,
      last: next.lastIndex,
      span: Math.max(1, Math.ceil(viewport / v.layout.stride) * v.layout.cols),
      direction: v.direction,
      current: v.current?.index ?? null,
      anchor: v.anchor?.index ?? null,
    });
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
        s.width === el.clientWidth && s.height === el.clientHeight ? s : { width: el.clientWidth, height: el.clientHeight },
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
      const unit = e.deltaMode === 1 ? LINE : e.deltaMode === 2 ? v.size.height : 1;
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
    if (old !== null && old.width > 0 && layout.width > 0 && (old.cols !== layout.cols || old.stride !== layout.stride)) {
      scrollReal(anchorAfterResize(old, layout, v.real));
      v.real = realScroll(boxRef.current?.scrollTop ?? 0, layout, v.size.height);
      if (old.cols !== layout.cols && v.current !== null) {
        const to = scrollToShow(v.current.index, layout, v.real, v.size.height);
        if (to !== null) scrollReal(to);
      }
    }
    update();
  }, [layout, size.height, update, scrollReal]);

  // Scrolling asked for by the store: to the current photo, or to the top.
  useLayoutEffect(() => {
    if (scroll === null) return;
    const v = live.current;
    if (scroll.to === "top") scrollReal(0);
    else if (v.current !== null) {
      const to = scrollToShow(v.current.index, v.layout, v.real, v.size.height);
      if (to !== null) scrollReal(to);
    }
    update();
  }, [scroll, scrollReal, update]);

  // The current photo or the anchor moved: their blocks are wanted and kept.
  useEffect(() => {
    update();
  }, [current, anchor, update]);

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
    current !== null && current.index >= range.firstIndex && current.index <= range.lastIndex
      ? current.index
      : range.firstIndex;
  const rows = [];
  for (let r = range.firstRow; r < range.endRow; r++) {
    const cells = [];
    const end = Math.min(count, (r + 1) * layout.cols);
    for (let i = r * layout.cols; i < end; i++) {
      const row = blocks.row(i);
      cells.push(
        <Tile
          key={i}
          index={i}
          row={row}
          x={PAD + (i - r * layout.cols) * layout.stride}
          side={layout.side}
          thumb={thumb}
          selected={row !== undefined && selected.has(row.id)}
          current={current !== null && current.index === i}
          tabbable={i === focusIndex}
          editTarget={editTarget}
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
        style={{ height: layout.side, transform: `translateY(${PAD + r * layout.stride}px)` }}
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
      <div className="ow-grid__content" style={{ height: `${layout.height}px` }}>
        <div ref={layerRef} className="ow-grid__layer">
          {rows}
        </div>
      </div>
    </div>
  );
}
