// The Histogram section (docs/04-browsing.md, "Histogram"): an 80px chart
// of four filled areas and the clipping line beneath it.

import type { Clipping } from "../../api/bindings/Clipping";
import type { Histogram } from "../../api/bindings/Histogram";
import { detailsText as t } from "../../text/details";
import { clippingParts } from "./facts";
import { useHistogram } from "./useDetail";

/** The chart's drawing height; it is stretched to 80px by CSS. */
const HEIGHT = 80;

/** The outline of one filled area, in a box `bins` wide and HEIGHT high. */
export function areaPath(counts: readonly number[], max: number): string {
  const n = counts.length;
  if (n === 0 || max <= 0) return "";
  const y = (c: number) => HEIGHT - (c / max) * HEIGHT;
  // Each bin is drawn at its centre; the area closes along the bottom.
  const points = counts.map((c, i) => `L${i + 0.5} ${y(c).toFixed(2)}`);
  return `M0 ${HEIGHT} L0 ${y(counts[0]!).toFixed(2)} ${points.join(" ")} L${n} ${y(counts[n - 1]!).toFixed(2)} L${n} ${HEIGHT} Z`;
}

function Chart({ histogram }: { histogram: Histogram }) {
  const channels = [histogram.l, histogram.r, histogram.g, histogram.b];
  const max = Math.max(0, ...channels.flat());
  const bins = histogram.l.length;
  return (
    <svg
      className="ow-histogram__chart"
      role="img"
      aria-label={t.histogramLabel}
      viewBox={`0 0 ${bins} ${HEIGHT}`}
      preserveAspectRatio="none"
    >
      <path className="ow-histogram__luma" d={areaPath(histogram.l, max)} />
      <path className="ow-histogram__red" d={areaPath(histogram.r, max)} />
      <path className="ow-histogram__green" d={areaPath(histogram.g, max)} />
      <path className="ow-histogram__blue" d={areaPath(histogram.b, max)} />
    </svg>
  );
}

interface HistogramSectionProps {
  id: number;
  thumbKey: string;
  clipping: Clipping | null;
}

/** Rendered only while the section is open, so a folded one asks nothing. */
export function HistogramSection({ id, thumbKey, clipping }: HistogramSectionProps) {
  const state = useHistogram(id, thumbKey);
  return (
    <div className="ow-histogram">
      {state.kind === "ready" && <Chart histogram={state.histogram} />}
      {state.kind === "loading" && <div className="ow-histogram__chart" aria-hidden="true" />}
      {state.kind === "failed" && <p className="ow-details__muted">{t.histogramFailed}</p>}
      {clipping !== null && (
        <p className="ow-histogram__clipping">
          {clippingParts(clipping).map((p) => (
            <span key={p.text} className={p.high ? "ow-histogram__high" : undefined}>
              {p.text}
            </span>
          ))}
        </p>
      )}
    </div>
  );
}
