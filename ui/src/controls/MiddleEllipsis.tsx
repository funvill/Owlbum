import { Tooltip } from "./Tooltip";
import "./MiddleEllipsis.css";

/** How many characters before the extension stay with the end. */
const keepBeforeExtension = 5;

/**
 * Splits a name into a start that may be cut and an end that is always
 * shown: the extension and a few characters before it.
 */
export function splitForEllipsis(text: string): [string, string] {
  const dot = text.lastIndexOf(".");
  const extension = dot > 0 ? text.length - dot : 0;
  const keep = extension + keepBeforeExtension;
  if (text.length <= keep) return ["", text];
  return [text.slice(0, text.length - keep), text.slice(text.length - keep)];
}

/**
 * A file name or path that, when it does not fit, is cut with an ellipsis
 * in the middle so the extension stays readable. The full text is in the
 * tooltip. docs/09-visual-design.md, "Typography".
 */
export function MiddleEllipsis({ text }: { text: string }) {
  const [start, end] = splitForEllipsis(text);
  return (
    <Tooltip text={text} describe={false}>
      <span className="ow-middle-ellipsis ow-selectable">
        <span className="ow-visually-hidden">{text}</span>
        <span className="ow-middle-ellipsis__start" aria-hidden="true">
          {start}
        </span>
        <span className="ow-middle-ellipsis__end" aria-hidden="true">
          {end}
        </span>
      </span>
    </Tooltip>
  );
}
