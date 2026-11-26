import type { MouseEvent } from "react";
import type { PhotoAt } from "../../api/bindings/PhotoAt";
import { useBrowseKeys } from "./browseKeys";
import { Grid, type GridProps } from "./Grid";
import { clickPhoto, menuPhoto } from "./selection";

export interface BrowseGridProps extends Pick<GridProps, "busy" | "onStar" | "onStack"> {
  /** Open the viewer on a photo: a double-click, Enter or Space (#22). */
  onOpen?: (at: PhotoAt) => void;
  /** The photo menu, after a right-click has selected the photo (#38). */
  onMenu?: (at: PhotoAt, event: MouseEvent) => void;
}

/**
 * The grid as the review screen uses it: clicks select (docs/04-browsing.md,
 * "Selection") and the review screen's keys work while it is shown.
 */
export function BrowseGrid({ onOpen, onMenu, ...rest }: BrowseGridProps) {
  useBrowseKeys({ onOpen });
  return (
    <Grid
      {...rest}
      onTileClick={(at, e) => clickPhoto(at, e)}
      onOpen={(at) => onOpen?.(at)}
      onMenu={(at, e) => {
        menuPhoto(at);
        onMenu?.(at, e);
      }}
    />
  );
}
