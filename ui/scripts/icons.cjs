// Builds ui/src/icons/paths.ts from lucide-static's icon-nodes.json.
// Usage, from ui/:
//   npm install --no-save lucide-static@1.81.0
//   node scripts/icons.cjs src/icons/paths.ts
// Then npm ci to drop lucide-static again. Keep notices/lucide.txt in step
// with the version.
const fs = require("fs");
const nodes = require("../node_modules/lucide-static/icon-nodes.json");
const version = require("../node_modules/lucide-static/package.json").version;

// Owlbum name -> Lucide name. Order follows docs/09-visual-design.md, "Icons".
const map = [
  // Carried over
  ["star", "star"], ["eye", "eye"], ["eye-off", "eye-off"], ["x", "x"], ["check", "check"],
  ["plus", "plus"], ["folder", "folder"], ["undo", "undo-2"], ["redo", "redo-2"],
  ["rotate-cw", "rotate-cw"], ["crop", "crop"], ["swap", "arrow-right-left"], ["trash", "trash"],
  ["copy", "copy"], ["image", "image"], ["grid", "layout-grid"], ["download", "download"],
  ["alert", "circle-exclamation-point"], ["info", "info"], ["lock", "lock"], ["sparkle", "sparkle"],
  // Arrows
  ["chevron-up", "chevron-up"], ["chevron-down", "chevron-down"], ["chevron-left", "chevron-left"],
  ["chevron-right", "chevron-right"], ["arrow-up", "arrow-up"], ["arrow-down", "arrow-down"],
  // New: browsing
  ["timeline", "timeline"], ["map", "map"], ["map-pin", "map-pin"], ["stack", "layers"],
  ["search", "search"], ["filter", "funnel"], ["sort", "arrow-up-down"], ["calendar", "calendar"],
  ["person", "user"], ["zoom-in", "zoom-in"], ["zoom-out", "zoom-out"], ["fit", "maximize"],
  ["details", "panel-right"],
  // New: video
  ["video", "video"], ["play", "play"], ["pause", "pause"], ["volume", "volume-2"],
  ["volume-off", "volume-x"],
  // New: editing
  ["rotate-ccw", "rotate-ccw"], ["flip-h", "move-horizontal"], ["flip-v", "move-vertical"],
  ["straighten", "ruler"], ["sliders", "sliders-horizontal"], ["reset", "refresh-ccw-dot"],
  // New: program
  ["star-filled", "star"], ["minus", "minus"], ["settings", "settings"], ["keyboard", "keyboard"],
  ["rescan", "refresh-cw"], ["tidy", "brush-cleaning"], ["close-album", "folder-x"],
  ["warning", "triangle-exclamation-point"], ["external", "external-link"], ["reveal", "folder-open"],
  ["path", "link"], ["more", "ellipsis"],
];

const num = (v) => Number(v);
const fmt = (v) => String(Math.round(v * 1000) / 1000);

function toPath([tag, a]) {
  switch (tag) {
    case "path": {
      // A path that starts with a relative moveto is absolute on its own,
      // but would be relative once joined after another: make it absolute.
      const d = a.d.trim();
      if (d[0] !== "m") return d;
      const num = /-?(?:\d+\.?\d*|\.\d+)(?:e[-+]?\d+)?/gi;
      num.lastIndex = 1;
      const x = num.exec(d);
      const y = num.exec(d);
      const rest = d.slice(num.lastIndex).replace(/^[\s,]+/, "");
      const tail = /^[-.\d]/.test(rest) ? `l${rest}` : rest;
      return `M${x[0]} ${y[0]}${tail}`;
    }
    case "line":
      return `M${a.x1} ${a.y1}L${a.x2} ${a.y2}`;
    case "circle": {
      const cx = num(a.cx), cy = num(a.cy), r = num(a.r);
      return `M${fmt(cx - r)} ${fmt(cy)}a${fmt(r)} ${fmt(r)} 0 1 0 ${fmt(2 * r)} 0a${fmt(r)} ${fmt(r)} 0 1 0 ${fmt(-2 * r)} 0`;
    }
    case "rect": {
      const x = num(a.x ?? 0), y = num(a.y ?? 0), w = num(a.width), h = num(a.height);
      const rx = num(a.rx ?? a.ry ?? 0), ry = num(a.ry ?? a.rx ?? 0);
      if (rx === 0) return `M${fmt(x)} ${fmt(y)}h${fmt(w)}v${fmt(h)}h${fmt(-w)}Z`;
      return (
        `M${fmt(x + rx)} ${fmt(y)}h${fmt(w - 2 * rx)}a${fmt(rx)} ${fmt(ry)} 0 0 1 ${fmt(rx)} ${fmt(ry)}` +
        `v${fmt(h - 2 * ry)}a${fmt(rx)} ${fmt(ry)} 0 0 1 ${fmt(-rx)} ${fmt(ry)}` +
        `h${fmt(-(w - 2 * rx))}a${fmt(rx)} ${fmt(ry)} 0 0 1 ${fmt(-rx)} ${fmt(-ry)}` +
        `v${fmt(-(h - 2 * ry))}a${fmt(rx)} ${fmt(ry)} 0 0 1 ${fmt(rx)} ${fmt(-ry)}Z`
      );
    }
    default:
      throw new Error(`unsupported element ${tag}`);
  }
}

let out = `// Generated from Lucide ${version} (lucide-static, ISC licence; see
// notices/lucide.txt). Each icon's elements are joined into one path.
// docs/09-visual-design.md, "Icons", lists the names and the Lucide
// drawing each one uses is named in the comment after it.

export const iconPaths = {\n`;
for (const [name, lucide] of map) {
  const n = nodes[lucide];
  if (!n) throw new Error(`missing ${lucide}`);
  const d = n.map(toPath).join("");
  out += `  ${JSON.stringify(name)}: ${JSON.stringify(d)}, // ${lucide}\n`;
}
out += `} as const;\n`;
fs.writeFileSync(process.argv[2], out);
console.log(`wrote ${map.length} icons`);
