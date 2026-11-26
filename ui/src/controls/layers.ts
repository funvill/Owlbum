// docs/09-visual-design.md, "Layering": popovers, the viewer, dialogs, menus,
// toasts and tooltips are drawn in one layer root at the end of the page, so
// no parent's clipping or stacking can trap them. Each sets its own z-index
// from the --z-* tokens.

const id = "ow-layer-root";

/** The layer root, made on first use as the last element of the body. */
export function layerRoot(): HTMLElement {
  let root = document.getElementById(id);
  if (root === null) {
    root = document.createElement("div");
    root.id = id;
    document.body.appendChild(root);
  }
  return root;
}
