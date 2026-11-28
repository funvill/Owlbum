// The Leaflet map behind the Map view (docs/04-browsing.md, "The map" and
// "Loading map tiles"). React owns the page around it; this owns the map,
// its markers, its clusters and the one tile layer.

import L from "leaflet";
import "./leafletGlobal";
import "leaflet.markercluster";
import "leaflet/dist/leaflet.css";
import "leaflet.markercluster/dist/MarkerCluster.css";
import type { MapPoint } from "../../api/bindings/MapPoint";
import { thumbUrl } from "../../api/media";
import { mapText } from "../../text/map";
import { editedHtml } from "../edit/drawing";
import { holdColourFilter } from "../edit/filters";
import { diffPoints } from "./points";

/** The first view, before any photo is placed. */
export const START = { lat: 20, lon: 0, zoom: 2 };
export const MIN_ZOOM = 1;
export const MAX_ZOOM = 19;
/** The first fit zooms in no further than this… */
export const FIT_MAX_ZOOM = 15;
/** …and leaves this much room at the edges. */
export const FIT_PADDING = 24;
/** Markers this close, in pixels, join into a cluster. */
export const CLUSTER_RADIUS = 48;
const PIN = 36;
/** Markers made and handed to the cluster group at a time. */
const SLICE = 20_000;
const BUBBLE = 40;

export interface PhotoMapEvents {
  /** A marker was clicked, with Ctrl (or Cmd) or not. */
  onPick(id: number, add: boolean): void;
  /** A tile failed, or the next one loaded after a failure. */
  onTiles(failing: boolean): void;
}

function escape(text: string): string {
  return text.replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);
}

/**
 * The marker's picture: the small thumbnail, drawn through the stored
 * edit's plan to cover the pin (docs/06-editing-and-file-actions.md,
 * "Where edits are shown").
 */
function pinPicture(point: MapPoint): string {
  const src = thumbUrl(point[3], "s");
  const plan = point[5];
  if (plan === null) return `<img src="${escape(src)}" alt="" draggable="false">`;
  return `<span class="ow-map-pin__edited">${editedHtml(plan, src, PIN, PIN, "cover")}</span>`;
}

function pinIcon(point: MapPoint, current: boolean): L.DivIcon {
  return L.divIcon({
    className: current ? "ow-map-pin ow-map-pin--current" : "ow-map-pin",
    html: pinPicture(point),
    iconSize: [PIN, PIN],
    iconAnchor: [PIN / 2, PIN / 2],
  });
}

function clusterIcon(cluster: L.MarkerCluster): L.DivIcon {
  const n = cluster.getChildCount();
  return L.divIcon({
    className: "ow-map-cluster",
    html: `<span aria-label="${escape(mapText.cluster(n))}">${n.toLocaleString("en-US")}</span>`,
    iconSize: [BUBBLE, BUBBLE],
    iconAnchor: [BUBBLE / 2, BUBBLE / 2],
  });
}

/** One map in one element. Call `destroy` when the element goes. */
export class PhotoMap {
  readonly map: L.Map;
  private readonly clusters: L.MarkerClusterGroup;
  private readonly markers = new Map<number, L.Marker>();
  private readonly points = new Map<number, MapPoint>();
  /** The colour filters the markers' edited pictures hold, by photo id. */
  private readonly filters = new Map<number, () => void>();
  private tiles: L.TileLayer | null = null;
  private failing = false;
  private current: number | null = null;
  /**
   * Whether the cluster group is still adding markers in chunks (#146):
   * 100,000 markers take a few seconds to cluster, so the library adds
   * them a slice at a time and the page stays responsive.
   */
  private loading = false;
  /** Points that arrived while loading, put on the map when it ends. */
  private waiting: readonly MapPoint[] | null = null;
  /** A pan to the current photo asked for while loading. */
  private followWaiting = false;
  /** `destroy` was called while loading: the map goes when the load ends. */
  private destroyed = false;
  private readonly settled: (() => void)[] = [];
  /** Inside `addLayers`, and whether the load ended within it. */
  private adding = false;
  /** How many markers the slice being clustered adds, and whether one is. */
  private loadSize = 0;
  private slicing = false;
  /** Points whose markers are still to be made, in slices. */
  private pending: MapPoint[] = [];
  private finished = false;

  constructor(
    element: HTMLElement,
    private readonly events: PhotoMapEvents,
  ) {
    this.map = L.map(element, {
      center: [START.lat, START.lon],
      zoom: START.zoom,
      minZoom: MIN_ZOOM,
      maxZoom: MAX_ZOOM,
      // The arrow keys step through the photos.
      keyboard: false,
      worldCopyJump: true,
      attributionControl: false,
    });
    // The credit is plain text: the page has no links to follow.
    L.control.attribution({ prefix: false }).addTo(this.map);
    this.clusters = L.markerClusterGroup({
      maxClusterRadius: CLUSTER_RADIUS,
      showCoverageOnHover: false,
      iconCreateFunction: clusterIcon,
      // Added in slices of about 100 ms, each followed by a pause, so a
      // whole album's markers do not hold the page up (issue 146).
      chunkedLoading: true,
      chunkInterval: 100,
      chunkDelay: 16,
      // The library still finishes its own work after the last report, so
      // what follows the load runs once it has returned.
      chunkProgress: (done: number, total: number) => {
        // The library also reports adds of its own (an empty one when the
        // group joins the map): only the end of the load started here counts.
        if (!this.slicing || total !== this.loadSize || done < total) return;
        if (this.adding) this.finished = true;
        else queueMicrotask(() => this.sliceDone());
      },
    });
    this.map.addLayer(this.clusters);
  }

  /** The markers now drawn, by id. */
  shown(): ReadonlyMap<number, MapPoint> {
    return this.points;
  }

  /** The marker drawn for photo `id`. */
  marker(id: number): L.Marker | undefined {
    return this.markers.get(id);
  }

  /** Resolves once every marker asked for is in the cluster group. */
  whenLoaded(): Promise<void> {
    if (!this.loading) return Promise.resolve();
    return new Promise((resolve) => this.settled.push(resolve));
  }

  /**
   * Puts the result's points on the map, keeping markers that still serve.
   * The map does not move. Points that arrive while markers are still
   * being added wait for that to end; the newest wins.
   */
  setPoints(next: readonly MapPoint[]): void {
    if (this.destroyed) return;
    if (this.loading) {
      this.waiting = next;
      return;
    }
    const { add, remove } = diffPoints(this.points, next);
    const gone = remove.map((id) => this.markers.get(id)).filter((m): m is L.Marker => m !== undefined);
    for (const id of remove) {
      this.markers.delete(id);
      this.points.delete(id);
      this.filters.get(id)?.();
      this.filters.delete(id);
    }
    this.clusters.removeLayers(gone);
    if (add.length === 0) return;
    for (const p of add) this.points.set(p[0], p);
    this.pending = add;
    this.loading = true;
    this.pump();
  }

  /**
   * Makes the markers of the next slice of points and hands them to the
   * cluster group. Making 100,000 markers at once held the page for over a
   * second, so they are made a slice at a time too (#146).
   */
  private pump(): void {
    if (this.destroyed) {
      this.loading = false;
      this.teardown();
      return;
    }
    const slice = this.pending.splice(0, SLICE);
    const fresh = slice.map((p) => this.makeMarker(p));
    this.loadSize = fresh.length;
    this.slicing = true;
    this.adding = true;
    this.clusters.addLayers(fresh);
    this.adding = false;
    if (this.finished) {
      this.finished = false;
      this.sliceDone();
    }
  }

  private makeMarker(p: MapPoint): L.Marker {
    const isCurrent = p[0] === this.current;
    const marker = L.marker([p[1], p[2]], {
      icon: pinIcon(p, isCurrent),
      title: p[4],
      alt: p[4],
      keyboard: false,
      zIndexOffset: isCurrent ? 1000 : 0,
    });
    marker.on("click", (e: L.LeafletMouseEvent) => {
      const ev = e.originalEvent;
      this.events.onPick(p[0], ev?.ctrlKey === true || ev?.metaKey === true);
    });
    this.markers.set(p[0], marker);
    const colour = p[5]?.colour ?? null;
    if (p[5] && colour !== null) this.filters.set(p[0], holdColourFilter(p[5].key, colour.svg.map(String).join(" ")));
    return marker;
  }

  /** The cluster group has every marker of the slice: the next slice, or the end of the load. */
  private sliceDone(): void {
    if (!this.slicing) return;
    this.slicing = false;
    if (this.pending.length > 0 && !this.destroyed) {
      setTimeout(() => this.pump(), 0);
      return;
    }
    this.loaded();
  }

  /** The load ended: tear down, or put on the points that waited. */
  private loaded(): void {
    this.loading = false;
    if (this.destroyed) {
      this.teardown();
      return;
    }
    const next = this.waiting;
    this.waiting = null;
    if (next !== null) this.setPoints(next);
    if (this.loading) return;
    if (this.followWaiting) {
      this.followWaiting = false;
      this.setCurrent(this.current, true);
    }
    for (const resolve of this.settled.splice(0)) resolve();
  }


  /** Fits every marker in view, without animation. Answers false when there was nothing to fit or no room. */
  fit(): boolean {
    const size = this.map.getSize();
    if (this.points.size === 0 || size.x === 0 || size.y === 0) return false;
    const bounds = L.latLngBounds([...this.points.values()].map((p) => [p[1], p[2]] as L.LatLngTuple));
    this.map.fitBounds(bounds, { padding: [FIT_PADDING, FIT_PADDING], maxZoom: FIT_MAX_ZOOM, animate: false });
    return true;
  }

  /**
   * Marks the current photo's marker. With `follow`, a marker off screen is
   * panned to at the same zoom, and a marker inside a cluster is shown.
   */
  setCurrent(id: number | null, follow: boolean): void {
    if (id !== this.current) {
      this.restyle(this.current, false);
      this.current = id;
      this.restyle(id, true);
    }
    if (!follow || id === null) return;
    if (this.loading) {
      // A marker still waiting to be added has no cluster to open yet.
      this.followWaiting = true;
      return;
    }
    const marker = this.markers.get(id);
    if (marker === undefined) return;
    const size = this.map.getSize();
    if (size.x === 0 || size.y === 0) return;
    if (!this.map.getBounds().contains(marker.getLatLng())) this.map.panTo(marker.getLatLng(), { animate: false });
    if (this.clusters.getVisibleParent(marker) !== marker) this.clusters.zoomToShowLayer(marker);
  }

  /**
   * Centres photo `id`'s marker ("Show on map"), once every marker asked
   * for is on the map. A marker inside a cluster is shown first. Answers
   * false when the map has no marker for it.
   */
  async centre(id: number): Promise<boolean> {
    await this.whenLoaded();
    const marker = this.markers.get(id);
    if (marker === undefined || this.destroyed) return false;
    const at = marker.getLatLng();
    if (this.clusters.getVisibleParent(marker) !== marker) {
      this.clusters.zoomToShowLayer(marker, () => this.map.setView(at, this.map.getZoom(), { animate: false }));
    } else {
      this.map.setView(at, this.map.getZoom(), { animate: false });
    }
    return true;
  }

  /** Creates or removes the tile layer. Tiles are requested only while it exists. */
  setTiles(on: boolean): void {
    if (on === (this.tiles !== null)) return;
    if (!on) {
      this.tiles!.remove();
      this.tiles = null;
      this.setFailing(false);
      return;
    }
    // The only tile address in the page (docs/08-desktop-and-release.md, "Map-tile consent").
    this.tiles = L.tileLayer("https://tile.openstreetmap.org/{z}/{x}/{y}.png", {
      maxZoom: MAX_ZOOM,
      attribution: mapText.attribution,
      referrerPolicy: "no-referrer",
    });
    this.tiles.on("tileerror", () => this.setFailing(true));
    this.tiles.on("tileload", () => this.setFailing(false));
    this.tiles.addTo(this.map);
  }

  /** Whether the tile layer exists. */
  hasTiles(): boolean {
    return this.tiles !== null;
  }

  /** Re-reads the element's size after it changed. */
  resized(): void {
    this.map.invalidateSize({ animate: false });
  }

  destroy(): void {
    if (this.destroyed) return;
    this.destroyed = true;
    // The library's next slice needs the map, so it stays until the load ends.
    if (!this.loading) this.teardown();
  }

  private teardown(): void {
    for (const resolve of this.settled.splice(0)) resolve();
    this.map.remove();
    for (const release of this.filters.values()) release();
    this.filters.clear();
    this.markers.clear();
    this.points.clear();
    this.tiles = null;
  }

  private setFailing(failing: boolean): void {
    if (failing === this.failing) return;
    this.failing = failing;
    this.events.onTiles(failing);
  }

  private restyle(id: number | null, current: boolean): void {
    if (id === null) return;
    const marker = this.markers.get(id);
    const point = this.points.get(id);
    if (marker === undefined || point === undefined) return;
    marker.setIcon(pinIcon(point, current));
    marker.setZIndexOffset(current ? 1000 : 0);
  }
}
