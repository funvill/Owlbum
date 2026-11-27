import { timelineText } from "../../text/timeline";
import type { RailYear } from "./rail";

export interface MonthRailProps {
  years: readonly RailYear[];
  /** Whether every month fits; when not, only the years show. */
  months: boolean;
  /** The `YYYY-MM` of the month holding the top of the view, or null. */
  at: string | null;
  /** Scrolls group `g`'s heading to the top. */
  onJump(group: number): void;
}

/**
 * The timeline's month rail (docs/04-browsing.md, "Month rail"): a 44 px
 * column at the right of the grid box, scrolling by itself when even the
 * years are too tall, listing the years in bold and the
 * months that have photos. The month in view is marked in `--accent`.
 */
export function MonthRail({ years, months, at, onJump }: MonthRailProps) {
  const t = timelineText.rail;
  const year = at === null ? null : Number(at.slice(0, 4));
  return (
    <nav className="ow-rail" aria-label={t.label}>
      <ul className="ow-rail__list">
        {years.map((y) => {
          // Collapsed to years, the year in view is the one marked.
          const yearOn = !months && y.year === year;
          return (
            <li key={y.year}>
              <button
                type="button"
                className={`ow-rail__item ow-rail__year${yearOn ? " ow-rail__item--on" : ""}`}
                aria-current={yearOn ? "true" : undefined}
                onClick={() => onJump(y.group)}
              >
                {y.year}
              </button>
              {months && (
                <ul className="ow-rail__list">
                  {y.months.map((m) => (
                    <li key={m.key}>
                      <button
                        type="button"
                        className={`ow-rail__item${m.key === at ? " ow-rail__item--on" : ""}`}
                        aria-label={t.monthName(y.year, m.month)}
                        aria-current={m.key === at ? "true" : undefined}
                        onClick={() => onJump(m.group)}
                      >
                        {t.month(m.month)}
                      </button>
                    </li>
                  ))}
                </ul>
              )}
            </li>
          );
        })}
      </ul>
    </nav>
  );
}
