import { useEffect } from "react";
import { focusOf, matchKey } from "../../keys";

// F6 and Shift+F6 move focus between the window's regions
// (docs/09-visual-design.md, "Accessibility baseline"): the top bar, the
// filter bar, the grid, the preview and the details panel. A region is any
// element marked `data-region`; their order is the page's order, which is
// the reading order. Focus returns to where it last was in a region, or to
// the grid's current cell, or to the region's first control.

/** The selector for a region. */
const REGION = "[data-region]";

/** What can take focus with Tab. */
const TABBABLE =
  'button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), a[href], [tabindex]:not([tabindex="-1"])';

/** Where focus last was in each region. */
const last = new WeakMap<Element, HTMLElement>();

function shown(el: Element): boolean {
  return el.closest("[hidden], [inert], [aria-hidden='true']") === null;
}

/** The regions on the page now, in reading order. */
export function regions(root: ParentNode = document): HTMLElement[] {
  return [...root.querySelectorAll<HTMLElement>(REGION)].filter(shown);
}

/** The element in `region` that should take focus when F6 arrives there. */
function entry(region: HTMLElement): HTMLElement {
  const before = last.get(region);
  if (before !== undefined && before.isConnected && region.contains(before) && shown(before)) return before;
  const cell = region.querySelector<HTMLElement>('[role="grid"] [tabindex="0"]');
  if (cell !== null) return cell;
  const first = [...region.querySelectorAll<HTMLElement>(TABBABLE)].find(shown);
  if (first !== undefined) return first;
  if (!region.hasAttribute("tabindex")) region.tabIndex = -1;
  return region;
}

/** Moves focus to the next region (`step` 1) or the previous one (-1), wrapping around. */
export function moveRegion(step: 1 | -1): void {
  const all = regions();
  if (all.length === 0) return;
  const active = document.activeElement;
  const at = all.findIndex((r) => active !== null && r.contains(active));
  const next = at === -1 ? (step === 1 ? 0 : all.length - 1) : (at + step + all.length) % all.length;
  const target = all[next];
  if (target !== undefined) entry(target).focus();
}

/** Whether a modal layer is open: the regions are behind it. */
function modalOpen(): boolean {
  return document.querySelector('[aria-modal="true"]') !== null;
}

/**
 * Listens for F6 and Shift+F6 while mounted, and remembers where focus was
 * in each region. Not while a dialog or the viewer is open.
 */
export function useRegionKeys(): void {
  useEffect(() => {
    const onFocusIn = (e: FocusEvent) => {
      const target = e.target;
      if (!(target instanceof HTMLElement)) return;
      const region = target.closest(REGION);
      if (region !== null && region !== target) last.set(region, target);
    };
    const onKeyDown = (e: KeyboardEvent) => {
      const focus = modalOpen() ? "dialog" : focusOf(e.target);
      const match = matchKey(e, { place: "browse", focus });
      if (match === null || (match.id !== "app.next_region" && match.id !== "app.previous_region")) return;
      e.preventDefault();
      if (match.act) moveRegion(match.id === "app.next_region" ? 1 : -1);
    };
    document.addEventListener("focusin", onFocusIn);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("focusin", onFocusIn);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, []);
}
