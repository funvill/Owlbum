// docs/09-visual-design.md, "Accessibility baseline": page tests run axe and
// fail on any serious finding. jsdom does no layout, so the colour-contrast
// rule cannot run here; the token tests check contrast from the values.

import axe from "axe-core";

/** The serious and critical axe findings inside `root`, as readable lines. */
export async function seriousFindings(root: Element = document.body): Promise<string[]> {
  const result = await axe.run(root, {
    rules: {
      "color-contrast": { enabled: false },
      // A component rendered on its own is not inside the page's landmarks.
      region: { enabled: false },
    },
  });
  return result.violations
    .filter((v) => v.impact === "serious" || v.impact === "critical")
    .map((v) => `${v.id}: ${v.help} (${v.nodes.map((n) => n.target.join(" ")).join(", ")})`);
}
