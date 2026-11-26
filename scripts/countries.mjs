// Builds crates/owlbum-core/data/countries.bin, the bundled country outlines
// behind the Country row and the country search (docs/04-browsing.md, "How
// typed fields save"; docs/01-architecture.md, "Dependencies chosen").
//
//   node scripts/countries.mjs                 download the pinned file, write the data
//   node scripts/countries.mjs --input F.geojson   use a file already downloaded
//   node scripts/countries.mjs --scale 110m --tolerance 0 --out other.bin
//
// The source is Natural Earth's admin-0 countries (public domain), pinned to
// a release and checked by SHA-256. Each ring is simplified with
// Douglas-Peucker, coordinates are rounded to 0.0001 degree (about 11 m) and
// written as zigzag varint deltas. The download goes to the system temp
// folder, never into the repository.
//
// File format, little-endian:
//   "OWLCTRY1"
//   u16 country count
//   per country:
//     u8 name length, the name in UTF-8
//     i32 label longitude, i32 label latitude   (units of 0.0001 degree)
//     varint ring count
//     per ring: varint point count, then the points as zigzag varint deltas
//     (longitude then latitude) from the previous point, the first from 0,0
// Rings are not closed (the last point is not repeated). A country's outer
// rings and holes are all listed; a point is inside when it is inside an odd
// number of them.

import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const RELEASE = "v5.1.2";
const SOURCES = {
  "110m": "6866c877d39cba9c357620878839b336d569f8c662d3cfab4cb1dbe2d39c977f",
  "50m": "3e458fc036ad0a66411f2c1e6cac49c5d7bfb81cb1123bc513b22511a2b7fdeb",
  "10m": "239eec57ac17f100a11e2536cffc56752c318b50ae765b0918ff7aab4ce8f255",
};
// The scale and simplification chosen by measurement (#25).
const DEFAULT_SCALE = "10m";
const DEFAULT_TOLERANCE = 0.01;
const UNIT = 10000; // per degree

// Natural Earth's English names (NAME_EN), except where it gives a formal
// name that nobody types or, for the Spratly Islands, a wrong one. Keyed by
// the feature's ADMIN field.
const RENAME = {
  China: "China",
  "Northern Cyprus": "Northern Cyprus",
  "Spratly Islands": "Spratly Islands",
};

function sourceUrl(scale) {
  return `https://raw.githubusercontent.com/nvkelso/natural-earth-vector/${RELEASE}/geojson/ne_${scale}_admin_0_countries.geojson`;
}

async function readSource(scale, input) {
  let bytes;
  if (input !== undefined) {
    bytes = readFileSync(input);
  } else {
    const cache = join(tmpdir(), `owlbum-ne-${RELEASE}-${scale}.geojson`);
    if (existsSync(cache)) {
      bytes = readFileSync(cache);
    } else {
      const response = await fetch(sourceUrl(scale));
      if (!response.ok) throw new Error(`Download failed: ${response.status} ${sourceUrl(scale)}`);
      bytes = Buffer.from(await response.arrayBuffer());
      writeFileSync(cache, bytes);
    }
  }
  const sha = createHash("sha256").update(bytes).digest("hex");
  if (sha !== SOURCES[scale]) {
    throw new Error(`The ${scale} source does not match the pinned SHA-256 (got ${sha}).`);
  }
  return JSON.parse(bytes.toString("utf8"));
}

/** Douglas-Peucker on an open ring, keeping its first and farthest points. */
function simplify(ring, tolerance) {
  if (tolerance <= 0 || ring.length <= 4) return ring;
  const keep = new Uint8Array(ring.length);
  // Split the closed ring at the point farthest from the first, so both
  // halves are open polylines with fixed ends.
  let far = 0;
  let best = -1;
  for (let i = 1; i < ring.length; i++) {
    const d = (ring[i][0] - ring[0][0]) ** 2 + (ring[i][1] - ring[0][1]) ** 2;
    if (d > best) {
      best = d;
      far = i;
    }
  }
  keep[0] = 1;
  keep[far] = 1;
  const closed = [...ring, ring[0]];
  const stack = [
    [0, far],
    [far, ring.length],
  ];
  const t2 = tolerance * tolerance;
  while (stack.length > 0) {
    const [a, b] = stack.pop();
    if (b - a < 2) continue;
    const [ax, ay] = closed[a];
    const [bx, by] = closed[b];
    const dx = bx - ax;
    const dy = by - ay;
    const len2 = dx * dx + dy * dy;
    let index = -1;
    let max = t2;
    for (let i = a + 1; i < b; i++) {
      const [px, py] = closed[i];
      let d;
      if (len2 === 0) {
        d = (px - ax) ** 2 + (py - ay) ** 2;
      } else {
        const t = Math.max(0, Math.min(1, ((px - ax) * dx + (py - ay) * dy) / len2));
        d = (px - ax - t * dx) ** 2 + (py - ay - t * dy) ** 2;
      }
      if (d > max) {
        max = d;
        index = i;
      }
    }
    if (index >= 0) {
      keep[index] = 1;
      stack.push([a, index], [index, b]);
    }
  }
  const kept = ring.filter((_, i) => keep[i] === 1);
  // A small island that would collapse to a sliver keeps its outline.
  return kept.length >= 4 ? kept : ring;
}

/** Even-odd over every ring, as the Rust lookup does. */
function insideAny(rings, [x, y]) {
  let inside = false;
  for (const ring of rings) {
    for (let i = 0, j = ring.length - 1; i < ring.length; j = i++) {
      const [xi, yi] = ring[i];
      const [xj, yj] = ring[j];
      if (yi > y !== yj > y && x < ((xj - xi) * (y - yi)) / (yj - yi) + xi) inside = !inside;
    }
  }
  return inside;
}

/**
 * A central point for a country whose label point lies at sea (Natural
 * Earth puts some between islands): the middle of the widest stretch of
 * land across the largest ring, at that ring's middle latitude.
 */
function innerPoint(rings) {
  const area = (r) => Math.abs(r.reduce((s, [x, y], i) => s + x * r[(i + 1) % r.length][1] - r[(i + 1) % r.length][0] * y, 0));
  const ring = rings.reduce((a, b) => (area(b) > area(a) ? b : a));
  const ys = ring.map(([, y]) => y);
  const y = Math.round((Math.min(...ys) + Math.max(...ys)) / 2) + 0.5;
  const xs = [];
  for (let i = 0, j = ring.length - 1; i < ring.length; j = i++) {
    const [xi, yi] = ring[i];
    const [xj, yj] = ring[j];
    if (yi > y !== yj > y) xs.push(((xj - xi) * (y - yi)) / (yj - yi) + xi);
  }
  xs.sort((a, b) => a - b);
  let best = [xs[0], xs[1]];
  for (let i = 0; i + 1 < xs.length; i += 2) if (xs[i + 1] - xs[i] > best[1] - best[0]) best = [xs[i], xs[i + 1]];
  return [Math.round((best[0] + best[1]) / 2), Math.round(y - 0.5)];
}

/** The outlines as `{ name, label: [lon, lat], rings: [[lon, lat][]] }`, in units. */
export function buildCountries(geojson, tolerance) {
  const countries = [];
  for (const feature of geojson.features) {
    const p = feature.properties;
    const name = RENAME[p.ADMIN] ?? p.NAME_EN;
    const g = feature.geometry;
    const polygons = g.type === "Polygon" ? [g.coordinates] : g.coordinates;
    const rings = [];
    for (const polygon of polygons) {
      for (const raw of polygon) {
        const open = raw.slice(0, raw.length - 1);
        const ring = [];
        for (const [x, y] of simplify(open, tolerance)) {
          const q = [Math.round(x * UNIT), Math.round(y * UNIT)];
          const last = ring[ring.length - 1];
          if (last === undefined || last[0] !== q[0] || last[1] !== q[1]) ring.push(q);
        }
        while (ring.length > 1 && ring[0][0] === ring[ring.length - 1][0] && ring[0][1] === ring[ring.length - 1][1]) {
          ring.pop();
        }
        if (ring.length >= 3) rings.push(ring);
      }
    }
    const label = [Math.round(p.LABEL_X * UNIT), Math.round(p.LABEL_Y * UNIT)];
    const atSea = !insideAny(rings, label);
    countries.push({ name, label: atSea ? innerPoint(rings) : label, labelMoved: atSea, rings });
  }
  countries.sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));
  return countries;
}

function varint(out, value) {
  let v = value >>> 0;
  while (v >= 0x80) {
    out.push((v & 0x7f) | 0x80);
    v >>>= 7;
  }
  out.push(v);
}

const zigzag = (n) => ((n << 1) ^ (n >> 31)) >>> 0;

export function encode(countries) {
  const out = [...Buffer.from("OWLCTRY1", "ascii")];
  out.push(countries.length & 0xff, countries.length >> 8);
  for (const c of countries) {
    const name = Buffer.from(c.name, "utf8");
    if (name.length > 255) throw new Error(`Name too long: ${c.name}`);
    out.push(name.length, ...name);
    const label = Buffer.alloc(8);
    label.writeInt32LE(c.label[0], 0);
    label.writeInt32LE(c.label[1], 4);
    out.push(...label);
    varint(out, c.rings.length);
    for (const ring of c.rings) {
      varint(out, ring.length);
      let px = 0;
      let py = 0;
      for (const [x, y] of ring) {
        varint(out, zigzag(x - px));
        varint(out, zigzag(y - py));
        px = x;
        py = y;
      }
    }
  }
  return Buffer.from(out);
}

function parseArgs(argv) {
  const args = { scale: DEFAULT_SCALE, tolerance: DEFAULT_TOLERANCE, input: undefined, out: undefined };
  for (let i = 0; i < argv.length; i += 2) {
    const [flag, value] = [argv[i], argv[i + 1]];
    if (flag === "--scale") args.scale = value;
    else if (flag === "--tolerance") args.tolerance = Number(value);
    else if (flag === "--input") args.input = value;
    else if (flag === "--out") args.out = value;
    else throw new Error(`Unknown option ${flag}`);
  }
  if (!(args.scale in SOURCES)) throw new Error(`Unknown scale ${args.scale}`);
  return args;
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
  const out = args.out ?? join(root, "crates/owlbum-core/data/countries.bin");
  const countries = buildCountries(await readSource(args.scale, args.input), args.tolerance);
  const bytes = encode(countries);
  mkdirSync(dirname(out), { recursive: true });
  writeFileSync(out, bytes);
  const points = countries.reduce((n, c) => n + c.rings.reduce((m, r) => m + r.length, 0), 0);
  const rings = countries.reduce((n, c) => n + c.rings.length, 0);
  console.log(
    `${out}: ${countries.length} countries, ${rings} rings, ${points} points, ${bytes.length} bytes ` +
      `(Natural Earth ${RELEASE} ${args.scale}, tolerance ${args.tolerance} degree)`,
  );
  const moved = countries.filter((c) => c.labelMoved).map((c) => c.name);
  if (moved.length > 0) console.log(`Central point moved onto land: ${moved.join(", ")}`);
}

if (process.argv[1] !== undefined && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  await main();
}
