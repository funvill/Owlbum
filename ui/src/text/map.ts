// Text shown by the map (src/features/map). docs/04-browsing.md, "Map view".

/** A whole number with digit grouping: 2,140. */
function group(n: number): string {
  return n.toLocaleString("en-US");
}

export const mapText = {
  /** The map stage's accessible name. */
  region: "Map of the photos",
  /** The note above the map, shown only when some photos have no place. */
  unlocated: (n: number) => (n === 1 ? "1 photo has no location" : `${group(n)} photos have no location`),
  /** The empty state when no photo of the result has a place (docs/09-visual-design.md, "Empty states"). */
  empty: "No photos with a location",
  /** The first-use card (docs/04-browsing.md, "Loading map tiles"). */
  consent: {
    text: "The map is downloaded from OpenStreetMap over the internet. This shows OpenStreetMap roughly where you are looking. Load maps from the internet?",
    load: "Load maps",
    notNow: "Not now",
  },
  /** The switch once the question is answered. */
  toggle: {
    online: "Map: online",
    offline: "Map: offline",
    onlineTip: "Stop loading maps from the internet",
    offlineTip: "Load maps from the internet",
  },
  /** The pill shown when a tile fails. */
  offline: "Map unavailable offline",
  /** The tile layer's credit. */
  attribution: "© OpenStreetMap contributors",
  /** The toast when `maps_online` could not be saved. */
  saveFailed: "Couldn't change the map setting.",
  /** A cluster bubble's label. */
  cluster: (n: number) => `${group(n)} photos here. Zoom in`,
};
