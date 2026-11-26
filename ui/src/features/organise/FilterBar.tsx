import type { Query } from "../../api/bindings/QueryShape";
import { SelectField, type SelectOption } from "../../controls";
import { organiseText as t } from "../../text/organise";
import { useGridStore } from "../grid";
import "./FilterBar.css";

type Sort = Query["sort"];

const SORTS: SelectOption[] = (Object.keys(t.sorts) as Sort[]).map((value) => ({ value, label: t.sorts[value] }));

/**
 * The filter bar (docs/05-organising.md, "The filter bar"): a search
 * landmark above the body while an album with photos is open. It holds the
 * Sort control (#29); Stack similar, the search box, the Filters button and
 * the count join it with their issues.
 *
 * A new sort re-runs the query with the selection kept; the grid then
 * scrolls as for any change the user makes (to the current photo, or to
 * the top). In the timeline view the order is always by date, so the
 * control is disabled there.
 */
export function FilterBar() {
  const query = useGridStore((s) => s.query);
  const timeline = query.view === "timeline";
  return (
    <div className="ow-filter-bar" role="search" aria-label={t.bar.label}>
      <SelectField
        label={t.bar.sort}
        value={timeline ? "date" : query.sort}
        options={SORTS}
        disabled={timeline}
        onChange={(sort) => {
          const grid = useGridStore.getState();
          grid.setQuery({ ...grid.query, sort: sort as Sort });
        }}
      />
    </div>
  );
}
