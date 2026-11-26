import { iconPaths } from "./paths";

/** The name of an icon in the set. Any other name is a type error. */
export type IconName = keyof typeof iconPaths;

/** Every icon name, in the order of the design document. */
export const iconNames = Object.keys(iconPaths) as IconName[];

/** The two icons drawn filled, for their "on" state. */
const filled: ReadonlySet<IconName> = new Set<IconName>(["star-filled", "play"]);

/** 14 px in small buttons and badges, 16 px by default, 20 px in empty states and banners. */
export type IconSize = "sm" | "md" | "lg";

const pixels: Record<IconSize, number> = { sm: 14, md: 16, lg: 20 };

interface IconProps {
  name: IconName;
  size?: IconSize;
  className?: string;
}

/**
 * One icon from the set, drawn inline in the text colour and hidden from
 * assistive technology: the control around it carries the name.
 * docs/09-visual-design.md, "Icons".
 */
export function Icon({ name, size = "md", className }: IconProps) {
  const d: string | undefined = iconPaths[name];
  if (d === undefined) throw new Error(`There is no icon named "${String(name)}".`);
  const px = pixels[size];
  const fill = filled.has(name);
  return (
    <svg
      className={className === undefined ? "ow-icon" : `ow-icon ${className}`}
      width={px}
      height={px}
      viewBox="0 0 24 24"
      fill={fill ? "currentColor" : "none"}
      stroke="currentColor"
      strokeWidth={1.7}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
    >
      <path d={d} />
    </svg>
  );
}
