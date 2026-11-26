import {
  cloneElement,
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type FocusEvent,
  type PointerEvent,
  type ReactElement,
} from "react";
import { createPortal } from "react-dom";
import { layerRoot } from "./layers";
import { lastInputWasKeyboard } from "./modality";
import { placeWithin, viewSize, type Point } from "./place";
import { tooltipDelayMs } from "./timing";
import "./Tooltip.css";

/** The one attribute the tooltip adds to the element it explains. */
interface TriggerProps {
  "aria-describedby"?: string;
}

interface TooltipProps {
  text: string;
  /** The one element it explains. */
  children: ReactElement<TriggerProps>;
  /**
   * Whether the text also describes the element to assistive technology.
   * Off for icon buttons, whose name already says it.
   */
  describe?: boolean;
}

/**
 * A short explanation that appears after the pointer rests on an element
 * for 500 ms, or at once on keyboard focus, and goes when the pointer leaves,
 * on blur, on Escape or on any key press. docs/09-visual-design.md, "Tooltips".
 */
export function Tooltip({ text, children, describe = true }: TooltipProps) {
  const id = useId();
  const [anchor, setAnchor] = useState<DOMRect | null>(null);
  const timer = useRef<number | undefined>(undefined);

  const hide = useCallback(() => {
    window.clearTimeout(timer.current);
    setAnchor(null);
  }, []);

  useEffect(() => () => window.clearTimeout(timer.current), []);

  // Any key press hides it. Escape stops there, so it does not also close
  // the dialog or menu underneath.
  useEffect(() => {
    if (anchor === null) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") e.stopPropagation();
      hide();
    };
    document.addEventListener("keydown", onKey, true);
    return () => document.removeEventListener("keydown", onKey, true);
  }, [anchor, hide]);

  const own = children.props["aria-describedby"];
  // While it shows, the tooltip describes the element.
  const explained =
    describe && anchor !== null
      ? cloneElement(children, { "aria-describedby": [own, id].filter((v) => v !== undefined).join(" ") })
      : children;

  // The handlers sit on a wrapper with no box of its own (display: contents),
  // so the element keeps its own handlers and its place in the layout.
  return (
    <>
      <span
        className="ow-tooltip-anchor"
        onPointerEnter={(e: PointerEvent<HTMLElement>) => {
          const target = e.currentTarget.firstElementChild ?? e.currentTarget;
          window.clearTimeout(timer.current);
          timer.current = window.setTimeout(() => setAnchor(target.getBoundingClientRect()), tooltipDelayMs);
        }}
        onPointerLeave={hide}
        onPointerDown={hide}
        onFocus={(e: FocusEvent<HTMLElement>) => {
          // Focus given by a mouse click, or moved by code after one, does not show it.
          if (!lastInputWasKeyboard()) return;
          window.clearTimeout(timer.current);
          setAnchor(e.target.getBoundingClientRect());
        }}
        onBlur={hide}
      >
        {explained}
      </span>
      {anchor !== null && createPortal(<TooltipBox id={id} text={text} anchor={anchor} />, layerRoot())}
    </>
  );
}

/** The gap between an element and its tooltip: --space-3. */
const gap = 8;

function TooltipBox({ id, text, anchor }: { id: string; text: string; anchor: DOMRect }) {
  const ref = useRef<HTMLDivElement>(null);
  const [at, setAt] = useState<Point>({ x: anchor.left, y: anchor.bottom + gap });

  // Centre it under the element, then keep it inside the window.
  useLayoutEffect(() => {
    const box = ref.current;
    if (box === null) return;
    const size = { width: box.offsetWidth, height: box.offsetHeight };
    const below = anchor.bottom + gap;
    const wanted = {
      x: anchor.left + anchor.width / 2 - size.width / 2,
      y: below + size.height > window.innerHeight ? anchor.top - gap - size.height : below,
    };
    setAt(placeWithin(wanted, size, viewSize()));
  }, [anchor]);

  return (
    <div ref={ref} id={id} role="tooltip" className="ow-tooltip" style={{ left: at.x, top: at.y }}>
      {text}
    </div>
  );
}
