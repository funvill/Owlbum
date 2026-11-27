import { useEffect, type CSSProperties } from "react";
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
 * The shape the mark and the score toast size themselves against: the stage
 * they sit on is a size container, and with the shown photo's width over
 * height in `--_photo-aspect` the CSS works out 40 % of the smaller side of
 * the photo as fitted on that stage (owner's feedback, issue 168).
 */
export function photoShape(aspect: number): CSSProperties {
  return { "--_photo-aspect": aspect > 0 ? aspect : 1 } as CSSProperties;
}

/**
 * The mark that plays over the preview pane or the viewer's picture after a
 * key starred or hid a photo (docs/05-organising.md, "The mark"). It is
 * shown for 0.8 s; a new press restarts it. With reduced motion it does not
 * move or fade (the tokens make the animation a no-op). `aspect` is the
 * shown photo's width over height.
 */
export function FlagMark({ place, aspect }: { place: MarkPlace; aspect: number }) {
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
      style={photoShape(aspect)}
    >
      <Icon name={ICONS[mark.kind]} />
    </span>
  );
}
