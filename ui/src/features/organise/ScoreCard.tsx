import { useEffect, useId, useState } from "react";
import { api } from "../../api";
import type { ScoreBreakdown } from "../../api/bindings/ScoreBreakdown";
import { organiseText as t } from "../../text/organise";
import "./ScoreCard.css";

/** Signed points with a real minus sign (U+2212): "+28", "−10". */
export function signed(points: number): string {
  return points < 0 ? `−${-points}` : `+${points}`;
}

/** A total with a real minus sign when it is below zero. */
function plain(total: number): string {
  return total < 0 ? `−${-total}` : String(total);
}

/**
 * The score card at the top of the details panel (docs/05-organising.md,
 * "The score card"): "Photo score" and the total. Hovering or focusing it
 * opens "What made this score", one row per line. Read from `photo_score`
 * again whenever the photo's details are read again (`revision`).
 */
export function ScoreCard({ id, revision }: { id: number; revision: number }) {
  const [score, setScore] = useState<ScoreBreakdown | null>(null);
  const [open, setOpen] = useState(false);
  const popup = useId();

  useEffect(() => {
    let live = true;
    api()
      .invoke("photo_score", { id })
      .then((s) => live && setScore(s))
      .catch(() => live && setScore(null));
    return () => {
      live = false;
    };
  }, [id, revision]);

  if (score === null) return null;
  return (
    <div
      className="ow-score-card"
      tabIndex={0}
      aria-describedby={open ? popup : undefined}
      onMouseEnter={() => setOpen(true)}
      onMouseLeave={() => setOpen(false)}
      onFocus={() => setOpen(true)}
      onBlur={() => setOpen(false)}
      data-testid="score-card"
    >
      <span className="ow-score-card__title">{t.card.title}</span>
      <span className="ow-score-card__total">{plain(score.total)}</span>
      {!score.analysed && <span className="ow-score-card__reading">{t.card.reading}</span>}
      {open && (
        <div className="ow-score-card__popup" id={popup} role="tooltip">
          <div className="ow-score-card__heading">{t.card.breakdown}</div>
          <table className="ow-score-card__lines">
            <tbody>
              {score.lines.map((line) => (
                <tr key={line.label}>
                  <th scope="row">{line.label}</th>
                  <td className="ow-score-card__why">{line.why}</td>
                  <td className={`ow-score-card__points ow-score-card__points--${line.points < 0 ? "minus" : "plus"}`}>
                    {signed(line.points)}
                  </td>
                </tr>
              ))}
              {!score.analysed && (
                <tr>
                  <td colSpan={3} className="ow-score-card__why">
                    {t.card.notAnalysed}
                  </td>
                </tr>
              )}
              <tr className="ow-score-card__sum">
                <th scope="row">{t.card.total}</th>
                <td />
                <td className="ow-score-card__points">{plain(score.total)}</td>
              </tr>
            </tbody>
          </table>
          <div className="ow-score-card__footer">{t.card.footer}</div>
        </div>
      )}
    </div>
  );
}
