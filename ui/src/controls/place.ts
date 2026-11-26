// Keeping menus and tooltips inside the window. Pure, so it can be tested
// without layout.

export interface Point {
  x: number;
  y: number;
}

export interface Size {
  width: number;
  height: number;
}

/**
 * Where to put something of `size` that wants its top left corner at
 * `wanted`, moved left and up as needed to stay inside `view`. It never goes
 * past the top left corner, even when it is bigger than the window.
 */
export function placeWithin(wanted: Point, size: Size, view: Size): Point {
  const x = Math.max(0, Math.min(wanted.x, view.width - size.width));
  const y = Math.max(0, Math.min(wanted.y, view.height - size.height));
  return { x, y };
}

/** The window's size. */
export function viewSize(): Size {
  return { width: window.innerWidth, height: window.innerHeight };
}
