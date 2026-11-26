import { memo, useState, type CSSProperties, type MouseEvent } from "react";
import { thumbUrl } from "../../api";
import type { EditTarget } from "../../api/bindings/EditTarget";
import type { PhotoAt } from "../../api/bindings/PhotoAt";
import type { Row } from "../../api/bindings/Row";
import { Tooltip } from "../../controls";
import { Icon } from "../../icons";
import { gridText } from "../../text/grid";

/** What the page does with the pointer on a tile. Selection is #19's; the rest belong to their own parts. */
export interface TileActions {
  /** A click on the photo (not on its star or stack count). */
  onTileClick?: (at: PhotoAt, event: MouseEvent) => void;
  /** A double-click on the photo: open the viewer. */
  onOpen?: (at: PhotoAt) => void;
  /** A right-click: the photo menu. */
  onMenu?: (at: PhotoAt, event: MouseEvent) => void;
  /** The star button: star or unstar this photo only. */
  onStar?: (row: Row) => void;
  /** The stack count: open or fold the stack. */
  onStack?: (row: Row) => void;
}

/** Thumbnail addresses that failed to load, so a tile drawn again does not ask again. */
const failed = new Set<string>();
const FAILED_KEPT = 5000;

/** `m:ss` under an hour, `h:mm:ss` from an hour, rounded down to whole seconds. */
export function videoLength(ms: number): string {
  const all = Math.floor(ms / 1000);
  const h = Math.floor(all / 3600);
  const m = Math.floor((all % 3600) / 60);
  const s = String(all % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${s}` : `${m}:${s}`;
}

interface TileProps {
  index: number;
  /** Undefined until the row arrives: a placeholder. */
  row: Row | undefined;
  x: number;
  side: number;
  thumb: "s" | "m";
  selected: boolean;
  current: boolean;
  tabbable: boolean;
  editTarget: EditTarget;
  actions: TileActions;
}

/** One square of the grid (docs/04-browsing.md, "Tiles"). */
export const Tile = memo(function Tile({
  index,
  row,
  x,
  side,
  thumb,
  selected,
  current,
  tabbable,
  editTarget,
  actions,
}: TileProps) {
  const [, redraw] = useState(0);
  const style: CSSProperties = { width: side, height: side, transform: `translateX(${x}px)` };
  const classes = ["ow-tile"];
  if (selected) classes.push("ow-tile--selected");
  if (current) classes.push("ow-tile--current");

  if (row === undefined) {
    return (
      <div
        role="gridcell"
        aria-selected={selected}
        aria-label={gridText.loading}
        tabIndex={tabbable ? 0 : -1}
        className={[...classes, "ow-tile--placeholder"].join(" ")}
        style={style}
      />
    );
  }

  const at: PhotoAt = { id: row.id, index };
  const url = thumbUrl(row.thumb_key, thumb);
  const broken = row.thumb_key === "" || failed.has(url);
  const edited = row.edit_plan !== null;
  const face = row.stack !== null && row.stack.face ? row.stack : null;
  if (row.hidden) classes.push("ow-tile--hidden");
  if (row.star) classes.push("ow-tile--starred");
  const name = gridText.tileName({
    fileName: row.file_name,
    star: row.star,
    hidden: row.hidden,
    score: row.score,
    stack: face === null ? null : face.size,
    edited,
  });
  const stop = (e: MouseEvent) => e.stopPropagation();
  const animated = row.kind !== "video" && row.duration_ms !== null;

  return (
    <Tooltip text={row.rel_path}>
      <div
        role="gridcell"
        aria-selected={selected}
        aria-label={name}
        tabIndex={tabbable ? 0 : -1}
        className={classes.join(" ")}
        style={style}
        onClick={(e) => actions.onTileClick?.(at, e)}
        onDoubleClick={() => actions.onOpen?.(at)}
        onContextMenu={(e) => {
          if (actions.onMenu === undefined) return;
          e.preventDefault();
          actions.onMenu(at, e);
        }}
      >
        {broken ? (
          <span className={`ow-tile__missing${row.kind === "video" ? " ow-tile__missing--video" : ""}`}>
            <Icon name={row.kind === "video" ? "video" : "warning"} size="lg" />
            <span className="ow-tile__missing-name">{row.file_name}</span>
          </span>
        ) : (
          <img
            className="ow-tile__picture"
            src={url}
            alt=""
            decoding="async"
            draggable={false}
            onError={() => {
              if (failed.size >= FAILED_KEPT) failed.clear();
              failed.add(url);
              redraw((n) => n + 1);
            }}
          />
        )}

        <span className="ow-tile__corner ow-tile__corner--top-left">
          {row.hidden && (
            <span className="ow-tile__badge" role="img" aria-label={gridText.hidden}>
              <Icon name="eye-off" size="sm" />
            </span>
          )}
          {edited && (
            <span
              className={`ow-tile__edit ow-tile__edit--${editTarget}`}
              role="img"
              aria-label={editTarget === "file" ? gridText.editedFile : gridText.editedAlbum}
              title={editTarget === "file" ? gridText.editedFile : gridText.editedAlbum}
            />
          )}
        </span>

        {face !== null && (
          <span className="ow-tile__corner ow-tile__corner--top-right">
            {actions.onStack === undefined ? (
              <span className={`ow-pill ow-pill--${face.open ? "accent" : "badge"} ow-num ow-tile__stack`}>
                ×{face.size}
              </span>
            ) : (
              <button
                type="button"
                className={`ow-pill ow-pill--${face.open ? "accent" : "badge"} ow-num ow-tile__stack`}
                aria-label={face.open ? gridText.stackFold(face.size) : gridText.stackShow(face.size)}
                title={face.open ? gridText.stackFold(face.size) : gridText.stackShow(face.size)}
                tabIndex={-1}
                onClick={(e) => {
                  stop(e);
                  actions.onStack?.(row);
                }}
                onDoubleClick={stop}
              >
                ×{face.size}
              </button>
            )}
          </span>
        )}

        <span className="ow-tile__corner ow-tile__corner--bottom-left">
          {actions.onStar !== undefined ? (
            <button
              type="button"
              className={`ow-tile__star${row.star ? " ow-tile__star--on" : ""}`}
              aria-label={row.star ? gridText.unstar : gridText.star}
              tabIndex={-1}
              onClick={(e) => {
                stop(e);
                actions.onStar?.(row);
              }}
              onDoubleClick={stop}
            >
              <Icon name={row.star ? "star-filled" : "star"} size="sm" />
            </button>
          ) : (
            row.star && (
              <span className="ow-tile__star ow-tile__star--on" role="img" aria-label={gridText.starred}>
                <Icon name="star-filled" size="sm" />
              </span>
            )
          )}
          {row.kind === "video" && (
            <span className="ow-pill ow-pill--badge ow-num">
              <Icon name="play" size="sm" />
              {row.duration_ms !== null && videoLength(row.duration_ms)}
            </span>
          )}
          {animated && (
            <span className="ow-pill ow-pill--badge">{/\.gif$/i.test(row.file_name) ? "GIF" : "WebP"}</span>
          )}
        </span>

        <span className="ow-tile__corner ow-tile__corner--bottom-right">
          <span
            className="ow-pill ow-pill--badge ow-num"
            role="img"
            aria-label={row.score === null ? gridText.noScore : gridText.score(row.score)}
          >
            {row.score === null ? "–" : row.score}
          </span>
        </span>

        <span className="ow-tile__outline" aria-hidden="true" />
      </div>
    </Tooltip>
  );
});
