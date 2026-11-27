import { useEffect } from "react";
import { scoreToastMs } from "../../controls/timing";
import { photoShape } from "./FlagMark";
import { endScoreToast, useScoreToastStore, type ToastPlace } from "./nudge";
import "./ScoreToast.css";

/**
 * The score toast after a nudge (docs/05-organising.md, "The score
 * toast"): centred 50 % across and 45 % down, green for up and red for
 * down, showing the change alone ("+5"), fading over 1100 ms. Each press restarts it. With reduced motion it
 * does not fade (the tokens make the animation a no-op) and is removed
 * after the same time. It is as tall as the star mark, 40 % of the shown
 * photo's smaller side; `aspect` is the photo's width over height.
 */
export function ScoreToast({ place, aspect }: { place: ToastPlace; aspect: number }) {
  const toast = useScoreToastStore((s) => (s.toast?.place === place ? s.toast : null));
  const seq = toast?.seq;
  useEffect(() => {
    if (seq === undefined) return;
    const timer = setTimeout(() => endScoreToast(seq), scoreToastMs);
    return () => clearTimeout(timer);
  }, [seq]);
  if (toast === null) return null;
  return (
    <span
      key={toast.seq}
      className={`ow-score-toast ow-score-toast--${toast.delta > 0 ? "up" : "down"}`}
      role="status"
      data-testid="score-toast"
      style={photoShape(aspect)}
    >
      {toast.text}
    </span>
  );
}
