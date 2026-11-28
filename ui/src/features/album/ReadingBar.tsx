import { useEffect, useState } from "react";
import { ProgressBar } from "../../controls";
import { albumText } from "../../text/album";
import { readingLabel } from "./reading";
import { useAlbumStore } from "./session";

/** Shown while a scan runs (docs/04-browsing.md, "Reading bar"). */
export function ReadingBar() {
  const p = useAlbumStore((s) => s.reading);
  const start = useAlbumStore((s) => s.readStart);
  const [now, setNow] = useState(() => Date.now());
  const shown = p !== null;
  useEffect(() => {
    if (!shown) return;
    // The estimate moves on between events.
    const tick = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(tick);
  }, [shown]);
  if (p === null) return null;
  const known = p.phase === "reading" && p.found > 0;
  return (
    <div className="ow-reading" role="region" aria-label={albumText.readingRegion}>
      <ProgressBar
        label={readingLabel(p, start, now)}
        value={known ? p.done : undefined}
        max={known ? p.found : undefined}
      />
    </div>
  );
}
