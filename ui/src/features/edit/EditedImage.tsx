// The one component that draws an edited photo
// (docs/06-editing-and-file-actions.md, "How the page draws a plan"). The
// image source is any upright picture of the photo (a thumbnail or the
// full size): all have the photo's shape, so one plan fits each.

import type { CSSProperties, ReactEventHandler } from "react";
import type { EditPlan } from "../../api/bindings/EditPlan";
import { drawPlan, filterId, type Fit } from "./drawing";
import { useColourFilter } from "./filters";

export interface EditedImageProps {
  plan: EditPlan;
  src: string;
  /** The box to fill, in CSS pixels. */
  boxW: number;
  boxH: number;
  fit: Fit;
  className?: string;
  imgClassName?: string;
  onLoad?: ReactEventHandler<HTMLImageElement>;
  onError?: ReactEventHandler<HTMLImageElement>;
}

/** The style of the outer element and the `<img>` for a plan in a box. */
export function editedStyles(plan: EditPlan, boxW: number, boxH: number, fit: Fit): { outer: CSSProperties; img: CSSProperties } {
  const d = drawPlan(plan, boxW, boxH, fit);
  return {
    outer: { width: d.outerW, height: d.outerH, overflow: "hidden", position: "relative" },
    img: {
      position: "absolute",
      left: 0,
      top: 0,
      transformOrigin: "0 0",
      width: d.width,
      height: d.height,
      maxWidth: "none",
      transform: d.transform,
      ...(d.filter === null ? {} : { filter: `url(#${filterId(plan.key)})` }),
    },
  };
}

/** An edited picture: the plan applied with a CSS transform and an SVG colour filter. */
export function EditedImage({ plan, src, boxW, boxH, fit, className, imgClassName, onLoad, onError }: EditedImageProps) {
  const filter = plan.colour === null ? null : plan.colour.svg.map((v) => String(v)).join(" ");
  useColourFilter(filter === null ? null : plan.key, filter);
  const style = editedStyles(plan, boxW, boxH, fit);
  return (
    <div className={["ow-edited", className].filter(Boolean).join(" ")} style={style.outer} data-edit-key={plan.key}>
      <img
        className={imgClassName}
        src={src}
        alt=""
        decoding="async"
        draggable={false}
        style={style.img}
        onLoad={onLoad}
        onError={onError}
      />
    </div>
  );
}
