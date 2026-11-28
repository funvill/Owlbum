import { useEffect, useRef, useState } from "react";
import type { Filter } from "../../api/bindings/Filter";
import { Icon } from "../../icons";
import { organiseText as t } from "../../text/organise";
import { useGridStore } from "../grid";
import { FilterPanel } from "./FilterPanel";
import { setStackSimilar } from "./stacks";

/** Runs the query with a new filter, keeping the rest of it. */
export function setFilter(filter: Filter): void {
  const grid = useGridStore.getState();
  grid.setQuery({ ...grid.query, filter });
}

/**
 * The Filters button and the panel it drops (docs/05-organising.md, "The
 * filter panel"), in the top bar beside the search box (owner's feedback,
 * issue 196). The panel holds Stack similar too. It reads "Filters · N"
 * while N panel filters are on. Esc or a press outside closes the panel.
 */
export function FiltersButton() {
  const query = useGridStore((s) => s.query);
  const facets = useGridStore((s) => s.facets);
  const panelCount = useGridStore((s) => s.result?.panel_count ?? 0);
  const [open, setOpen] = useState(false);
  const button = useRef<HTMLButtonElement>(null);
  const area = useRef<HTMLDivElement>(null);

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

  // The panel never outlives the button's album.
  useEffect(() => () => void useGridStore.getState().wantFacets(false), []);

  return (
    <div
      ref={area}
      className="ow-filters"
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
        className={`ow-button ow-button--default ow-filters__button${panelCount > 0 ? " ow-filters__button--on" : ""}`}
        aria-expanded={open}
        onClick={() => show(!open)}
      >
        <span className="ow-button__label">{t.bar.filters(panelCount)}</span>
        <Icon name={open ? "chevron-up" : "chevron-down"} size="sm" />
      </button>
      {open && (
        <FilterPanel
          filter={query.filter}
          facets={facets}
          onChange={setFilter}
          stack={query.stack}
          onStack={setStackSimilar}
        />
      )}
    </div>
  );
}
