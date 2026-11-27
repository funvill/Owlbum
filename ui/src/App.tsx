import { Toasts } from "./controls";
import { Shell } from "./features/album";
import { ColourFilters } from "./features/edit";

/** The whole page. Features are composed here. */
export function App() {
  return (
    <>
      <Shell />
      <Toasts />
      <ColourFilters />
    </>
  );
}
