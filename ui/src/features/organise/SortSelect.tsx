import type { Query } from "../../api/bindings/QueryShape";
import { SelectField, type SelectOption } from "../../controls";
import { organiseText as t } from "../../text/organise";
import { useGridStore } from "../grid";

type Sort = Query["sort"];

/** The sorts, read when drawn so they are in the language in use. */
const sorts = (): SelectOption[] => (Object.keys(t.sorts) as Sort[]).map((value) => ({ value, label: t.sorts[value] }));

/**
 * The Sort control (docs/05-organising.md, "Sorting"). It sits beside the
 * view switch while the grid is the view, the way Day / Month sit there
 * for the timeline (owner's feedback, issue 182). A change re-runs the
 * query with the selection kept.
 */
export function SortSelect() {
  const sort = useGridStore((s) => s.query.sort);
  return (
    <SelectField
      label={t.bar.sort}
      value={sort}
      options={sorts()}
      onChange={(next) => {
        const state = useGridStore.getState();
        state.setQuery({ ...state.query, sort: next as Sort });
      }}
    />
  );
}
