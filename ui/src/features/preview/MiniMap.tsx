// The mini map (docs/04-browsing.md, "Mini map"): the small thumbnail at
// the stage's top-right, with the visible part outlined. Pressing or
// dragging on it centres the view there; its pointer events never reach the
// stage behind.

import { useRef, type PointerEvent as ReactPointerEvent, type SyntheticEvent } from "react";
import { previewText } from "../../text/preview";
import { miniMapSize, visibleRect, type Box, type View } from "./zoom";

export interface MiniMapProps {
  src: string;
  fitted: Box;
  stage: Box;
  view: View;
  /** Centre the view on this point of the picture, as fractions of its width and height. */
  onCentre: (point: { u: number; v: number }) => void;
}

const stop = (e: SyntheticEvent) => e.stopPropagation();
const unit = (n: number) => Math.min(1, Math.max(0, n));

export function MiniMap({ src, fitted, stage, view, onCentre }: MiniMapProps) {
  const size = miniMapSize(fitted);
  const rect = visibleRect(view, stage, fitted);
  const pressed = useRef<number | null>(null);

  const centre = (e: ReactPointerEvent<HTMLDivElement>) => {
    const box = e.currentTarget.getBoundingClientRect();
    if (box.width <= 0 || box.height <= 0) return;
    onCentre({ u: unit((e.clientX - box.left) / box.width), v: unit((e.clientY - box.top) / box.height) });
  };

  return (
    <div
      className="ow-minimap"
      role="img"
      aria-label={previewText.miniMap}
      style={{ width: size.width, height: size.height }}
      onPointerDown={(e) => {
        e.stopPropagation();
        if (e.button !== 0) return;
        pressed.current = e.pointerId;
        e.currentTarget.setPointerCapture?.(e.pointerId);
        centre(e);
      }}
      onPointerMove={(e) => {
        e.stopPropagation();
        if (pressed.current === e.pointerId) centre(e);
      }}
      onPointerUp={(e) => {
        e.stopPropagation();
        pressed.current = null;
      }}
      onPointerCancel={() => (pressed.current = null)}
      onDoubleClick={stop}
      onContextMenu={stop}
    >
      <img className="ow-minimap__img" src={src} alt="" draggable={false} />
      <div
        className="ow-minimap__view"
        style={{
          left: `${rect.left * 100}%`,
          top: `${rect.top * 100}%`,
          width: `${rect.width * 100}%`,
          height: `${rect.height * 100}%`,
        }}
      />
    </div>
  );
}
