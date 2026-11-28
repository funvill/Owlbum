import { Toasts } from "./controls";
import { Shell } from "./features/album";
import { ColourFilters } from "./features/edit";
import { TrashQuestion } from "./features/files";

/** The whole page. Features are composed here. */
export function App() {
  return (
    <>
      <Shell />
      <Toasts />
      <TrashQuestion />
      <ColourFilters />
    </>
  );
}
