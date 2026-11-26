import { Toasts } from "./controls";
import { Shell } from "./features/album";

/** The whole page. Features are composed here. */
export function App() {
  return (
    <>
      <Shell />
      <Toasts />
    </>
  );
}
