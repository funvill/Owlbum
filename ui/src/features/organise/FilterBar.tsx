import { useEffect, useRef, useState } from "react";
import type { Filter } from "../../api/bindings/Filter";
import type { Query } from "../../api/bindings/QueryShape";
import { Button, Checkbox, SelectField, Tooltip, useNarrowWindow, type SelectOption } from "../../controls";
import { Icon } from "../../icons";
import { organiseText as t } from "../../text/organise";
import { useGridStore } from "../grid";
import { FilterChip } from "./FilterChip";
import { FilterPanel } from "./FilterPanel";
import { QueryBox, clearQueryBox } from "./QueryBox";
import { setStackSimilar } from "./stacks";
import "./FilterBar.css";

type Sort = Query["sort"];

const SORTS: SelectOption[] = (Object.keys(t.sorts) as Sort[]).map((value) => ({ value, label: t.sorts[value] }));

/** Runs the query with a new filter, keeping the rest of it. */
function setFilter(filter: Filter): void {
  const grid = useGridStore.getState();
  grid.setQuery({ ...grid.query, filter });
}

/**
 * The filter bar (docs/05-organising.md, "The filter bar"): a search
 * landmark above the body while an album with photos is open. The search
 * box with its hint line (#32) sits at the left; at the right are the Sort
 * control (#29, shown only in the grid view), the Stack similar checkbox
 * (#131, the `stack_similar` setting), the Filters button with its panel,
 * and the count. A second row, "Active filters", shows the chips, "Showing
 * X of Y" and Clear filter while a filter is on (#31).
 *
 * Every number and chip comes from the result: the page never counts or
 * builds chip text itself. A change of sort or filter re-runs the query
 * with the selection kept; the grid then scrolls as for any change the
 * user makes. The timeline is always in date order and the map has no
 * order to see, so Sort is shown only in the grid (owner's feedback,
 * issue 166); the chosen sort is kept for when the grid comes back.
 */
export function FilterBar() {
  const query = useGridStore((s) => s.query);
  const result = useGridStore((s) => s.result);
  const facets = useGridStore((s) => s.facets);
  const [open, setOpen] = useState(false);
  const button = useRef<HTMLButtonElement>(null);
  const area = useRef<HTMLDivElement>(null);
  const grid = query.view === "grid";
  const panelCount = result?.panel_count ?? 0;
  const filtered = result?.filtered ?? false;
  // In a narrow window the count is the number alone (docs/09-visual-design.md, "The bars").
  const narrow = useNarrowWindow();

  const show = (on: boolean) => {
    setOpen(on);
    void useGridStore.getState().wantFacets(on);
  };

  useEffect(() => {
    if (!open) return;
    // A press outside the panel and its button closes it without moving focus.
    const press = (e: PointerEvent) => {
      if (area.current !== null && !area.current.contains(e.target as Node)) {
        setOpen(false);
        void useGridStore.getState().wantFacets(false);
      }
    };
    document.addEventListener("pointerdown", press);
    return () => document.removeEventListener("pointerdown", press);
  }, [open]);

  // The panel never outlives the bar's album.
  useEffect(() => () => void useGridStore.getState().wantFacets(false), []);

  return (
    <div className="ow-filter-bar" role="search" aria-label={t.bar.label} data-region="filters">
      <div className="ow-filter-bar__main">
        <QueryBox />
        <span className="ow-filter-bar__spacer" />
        {grid && (
          <SelectField
            label={t.bar.sort}
            value={query.sort}
            options={SORTS}
            onChange={(sort) => {
              const state = useGridStore.getState();
              state.setQuery({ ...state.query, sort: sort as Sort });
            }}
          />
        )}
        <Tooltip text={t.bar.stackTip}>
          <span className="ow-filter-bar__stack">
            <Checkbox label={t.bar.stack} checked={query.stack} onChange={setStackSimilar} />
          </span>
        </Tooltip>
        <div
          ref={area}
          className="ow-filter-bar__filters"
          onKeyDown={(e) => {
            if (e.key === "Escape" && open) {
              e.preventDefault();
              e.stopPropagation();
              show(false);
              button.current?.focus();
            }
          }}
        >
          <button
            ref={button}
            type="button"
            className={`ow-button ow-button--default ow-filter-bar__button${panelCount > 0 ? " ow-filter-bar__button--on" : ""}`}
            aria-expanded={open}
            onClick={() => show(!open)}
          >
            <span className="ow-button__label">{t.bar.filters(panelCount)}</span>
            <Icon name={open ? "chevron-up" : "chevron-down"} size="sm" />
          </button>
          {open && <FilterPanel filter={query.filter} facets={facets} onChange={setFilter} />}
        </div>
        {!filtered && result !== null && (
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
        )}
      </div>
      {filtered && result !== null && (
        <div className="ow-filter-bar__filtered" role="region" aria-label={t.filtered.label}>
          {result.chips.map((chip) => (
            <FilterChip key={chip.id} text={chip.text} onRemove={() => setFilter(chip.remove)} />
          ))}
          <span className="ow-filter-bar__spacer" />
          <span className="ow-filter-bar__showing" aria-live="polite">
            {t.filtered.showing(result.photos, result.total)}
          </span>
          <Button
            size="sm"
            onClick={() => {
              clearQueryBox();
              const grid = useGridStore.getState();
              grid.setQuery({ ...grid.query, filter: {}, search: null });
            }}
          >
            {t.filtered.clear}
          </Button>
        </div>
      )}
    </div>
  );
}
