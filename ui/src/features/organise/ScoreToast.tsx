import { useEffect } from "react";
import { scoreToastMs } from "../../controls/timing";
import { endScoreToast, useScoreToastStore, type ToastPlace } from "./nudge";
import "./ScoreToast.css";

/**
 * The score toast after a nudge (docs/05-organising.md, "The score
 * toast"): centred 50 % across and 45 % down, green for up and red for
 * down, fading over 1100 ms. Each press restarts it. With reduced motion it
 * does not fade (the tokens make the animation a no-op) and is removed
 * after the same time.
 */
export function ScoreToast({ place }: { place: ToastPlace }) {
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
    >
      {toast.text}
    </span>
  );
}
