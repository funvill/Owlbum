import type { MouseEvent, PointerEvent } from "react";
import type { PhotoAt } from "../../api/bindings/PhotoAt";
import { useBrowseKeys } from "./browseKeys";
import { Grid, type GridProps } from "./Grid";
import { clickPhoto, menuPhoto } from "./selection";

export interface BrowseGridProps extends Pick<GridProps, "busy" | "onStar" | "onStack" | "across"> {
  /** Open the viewer on a photo: a double-click, Enter or Space (#22). */
  onOpen?: (at: PhotoAt) => void;
  /** The photo menu, after a right-click has selected the photo (#38). */
  onMenu?: (at: PhotoAt, event: MouseEvent) => void;
  /** A pointer pressed on a tile: may become a drag out (#42). */
  onPress?: (at: PhotoAt, event: PointerEvent) => void;
}

/**
 * The grid as the review screen uses it: clicks select (docs/04-browsing.md,
 * "Selection") and the review screen's keys work while it is shown.
 */
export function BrowseGrid({ onOpen, onMenu, onPress, ...rest }: BrowseGridProps) {
  useBrowseKeys({ onOpen });
  return (
    <Grid
      {...rest}
      onTileClick={(at, e) => clickPhoto(at, e)}
      onOpen={(at) => onOpen?.(at)}
      onPress={(at, e) => onPress?.(at, e)}
      onMenu={(at, e) => {
        menuPhoto(at);
        onMenu?.(at, e);
      }}
    />
  );
}
