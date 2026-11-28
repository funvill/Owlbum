import { Button, useNarrowWindow } from "../../controls";
import { organiseText as t } from "../../text/organise";
import { useGridStore } from "../grid";
import { FilterChip } from "./FilterChip";
import { setFilter } from "./FiltersButton";
import { clearQueryBox } from "./QueryBox";
import "./FilterBar.css";

/** Empties the search box and clears the search text and every filter. */
function clearAll(): void {
  clearQueryBox();
  const grid = useGridStore.getState();
  grid.setQuery({ ...grid.query, filter: {}, search: null });
}

/** Clears the search text alone: the search chip's ×. */
function clearSearch(): void {
  clearQueryBox();
  const grid = useGridStore.getState();
  grid.setQuery({ ...grid.query, search: null });
}

/**
 * The filter bar (docs/05-organising.md, "The filter bar"): the row under
 * the top bar while an album with photos is open, starting just below the
 * search box (owner's feedback, issue 196). It shows the count, "2,140
 * photos"; while a filter or search is on, "Showing X of Y", Clear filter
 * and a chip for each, the search text first as "Photos with 'lake'". The
 * Filters button and Stack similar are in the top bar beside the search
 * box, Sort beside the view switch.
 *
 * Every number and chip comes from the result: the page never counts or
 * builds chip text itself. A change of filter re-runs the query with the
 * selection kept; the grid then scrolls as for any change the user makes.
 */
export function FilterBar() {
  const result = useGridStore((s) => s.result);
  const filtered = result?.filtered ?? false;
  // In a narrow window the count is the number alone (docs/09-visual-design.md, "The bars").
  const narrow = useNarrowWindow();

  return (
    <div className="ow-filter-bar" role="search" aria-label={t.bar.label} data-region="filters">
      {!filtered && result !== null && (
        <div className="ow-filter-bar__main">
          <span className="ow-filter-bar__count" title={narrow ? t.bar.count(result.total) : undefined}>
            {narrow ? (
              <>
                {t.bar.countShort(result.total)}
                <span className="ow-visually-hidden">{t.bar.countUnit(result.total)}</span>
              </>
            ) : (
              t.bar.count(result.total)
            )}
          </span>
        </div>
      )}
      {filtered && result !== null && (
        <div className="ow-filter-bar__main ow-filter-bar__filtered" role="region" aria-label={t.filtered.label}>
          <span className="ow-filter-bar__showing" aria-live="polite">
            {t.filtered.showing(result.photos, result.total)}
          </span>
          <Button size="sm" onClick={clearAll}>
            {t.filtered.clear}
          </Button>
          {result.chips.map((chip) => (
            <FilterChip
              key={chip.id}
              text={chip.text}
              onRemove={chip.id === SEARCH_CHIP ? clearSearch : () => setFilter(chip.remove)}
            />
          ))}
        </div>
      )}
    </div>
  );
}

/** The id Rust gives the search text's chip. */
export const SEARCH_CHIP = "search";
