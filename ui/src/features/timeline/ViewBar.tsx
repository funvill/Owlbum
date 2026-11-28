import type { Query } from "../../api/bindings/QueryShape";
import { Segmented, type SegmentOption } from "../../controls";
import { timelineText as t } from "../../text/timeline";
import { useSettingsStore } from "../app";
import { useGridStore } from "../grid";
import { SortSelect } from "../organise/SortSelect";
import "./ViewBar.css";

const VIEWS: SegmentOption[] = [
  { value: "grid", label: t.views.grid, icon: "grid" },
  { value: "timeline", label: t.views.timeline, icon: "timeline" },
  { value: "map", label: t.views.map, icon: "map" },
];

const UNITS: SegmentOption[] = [
  { value: "day", label: t.unit.day },
  { value: "month", label: t.unit.month },
];

/** Runs the query with `change`, keeping the rest of it (and the sort, for the other views). */
function change(patch: Partial<Query>): void {
  const grid = useGridStore.getState();
  grid.setQuery({ ...grid.query, ...patch });
}

/**
 * The bar at the top of the thumbnail column (docs/04-browsing.md, "View
 * switch"): Grid, Timeline and Map, and at the right the Sort control in
 * the grid (owner's feedback, issue 182) or the Day / Month switch in the
 * timeline. A choice re-runs the query, which keeps the current photo and
 * the selection, and is saved as the setting `view` or `timeline_unit`.
 * The timeline is always in date order and the map has no order to see,
 * so Sort is shown only in the grid; the chosen sort is kept for when the
 * grid comes back.
 */
export function ViewBar() {
  const view = useGridStore((s) => s.query.view);
  const asked = useGridStore((s) => s.query.timeline_unit);
  const used = useGridStore((s) => s.result?.unit);
  // Until the user picks, the unit the result actually used.
  const unit = asked === "auto" ? (used ?? "day") : asked;
  return (
    <div className="ow-view-bar">
      <Segmented
        as="tablist"
        label={t.views.label}
        value={view}
        options={VIEWS}
        onChange={(value) => {
          const next = value === "timeline" || value === "map" ? value : "grid";
          if (next === view) return;
          change({ view: next });
          void useSettingsStore.getState().update({ view: next });
        }}
      />
      {view === "grid" && <SortSelect />}
      {view === "timeline" && (
        <Segmented
          label={t.unit.label}
          value={unit}
          options={UNITS}
          onChange={(value) => {
            const next = value === "month" ? "month" : "day";
            if (next === asked) return;
            change({ timeline_unit: next });
            void useSettingsStore.getState().update({ timeline_unit: next });
          }}
        />
      )}
    </div>
  );
}
