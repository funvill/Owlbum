// The owl mark without its tile, in one colour (currentColor), with the eyes
// as rings. Drawn from an earlier draft logo; the approved icon is a picture
// (docs/09-visual-design.md, "Logo and app icon"), and no screen shows this.

interface OwlMarkProps {
  /** Width and height in pixels; 64 in empty states. */
  size?: number;
}

/** The owl mark, hidden from assistive technology. */
export function OwlMark({ size = 64 }: OwlMarkProps) {
  return (
    <svg
      className="ow-owl-mark"
      width={size}
      height={size}
      viewBox="140 129 744 744"
      fill="none"
      stroke="currentColor"
      aria-hidden="true"
      focusable="false"
    >
      <path d="M190 250 L512 392 L834 250" strokeWidth={56} strokeLinecap="round" strokeLinejoin="round" />
      <circle cx="348" cy="556" r="134" strokeWidth={52} />
      <circle cx="676" cy="556" r="134" strokeWidth={52} />
      <path d="M512 640 L556 700 L512 780 L468 700 Z" fill="currentColor" stroke="none" />
    </svg>
  );
}
