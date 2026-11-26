import { useEffect } from "react";
import { markMs } from "../../controls/timing";
import { Icon } from "../../icons";
import { organiseText as t } from "../../text/organise";
import { endMark, useMarkStore, type MarkKind, type MarkPlace } from "./flags";
import "./FlagMark.css";

const ICONS: Record<MarkKind, "star" | "star-filled" | "eye" | "eye-off"> = {
  starred: "star-filled",
  unstarred: "star",
  hidden: "eye-off",
  unhidden: "eye",
};

/**
 * The mark that plays over the preview pane or the viewer's picture after a
 * key starred or hid a photo (docs/05-organising.md, "The mark"). It is
 * shown for 0.8 s; a new press restarts it. With reduced motion it does not
 * move or fade (the tokens make the animation a no-op).
 */
export function FlagMark({ place }: { place: MarkPlace }) {
  const mark = useMarkStore((s) => (s.mark?.place === place ? s.mark : null));
  const seq = mark?.seq;
  useEffect(() => {
    if (seq === undefined) return;
    const timer = setTimeout(() => endMark(seq), markMs);
    return () => clearTimeout(timer);
  }, [seq]);
  if (mark === null) return null;
  return (
    <span
      key={mark.seq}
      className={`ow-flag-mark ow-flag-mark--${mark.kind}`}
      role="img"
      aria-label={t.mark[mark.kind]}
      data-testid="flag-mark"
    >
      <Icon name={ICONS[mark.kind]} />
    </span>
  );
}
