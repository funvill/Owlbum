// The decoded-image cache and the preload order (docs/04-browsing.md,
// "Image cache and preloading"). One cache is shared by the preview and the
// viewer; it holds decoded medium pictures so a step paints with no wait.

/** Photos preloaded in the direction of travel. */
export const AHEAD = 20;
/** Photos preloaded the other way. */
export const BEHIND = 5;
/** The nearest photos ahead, loaded before anything behind. */
export const LEAD = 3;
/** Background loads running at once. */
export const MAX_IN_FLIGHT = 4;
/** Decoded pictures kept. */
export const CAPACITY = 40;
/** Failed addresses remembered; the oldest is forgotten first. */
export const FAILED_KEPT = 500;
/** How long a failure blocks a retry. */
export const FAILED_FOR_MS = 15_000;

/**
 * The indexes to load around `index`, most wanted first: the next `LEAD`
 * places in the direction of travel, one behind, the rest ahead up to
 * `AHEAD`, then behind from 2 to `BEHIND`. Never `index` itself, a
 * duplicate, or anything outside `0 … count − 1`.
 */
export function preloadOrder(index: number, count: number, direction: 1 | -1): number[] {
  const order: number[] = [];
  const add = (offset: number) => {
    const i = index + offset * direction;
    if (i >= 0 && i < count && i !== index) order.push(i);
  };
  for (let k = 1; k <= Math.min(LEAD, AHEAD); k++) add(k);
  if (BEHIND >= 1) add(-1);
  for (let k = LEAD + 1; k <= AHEAD; k++) add(k);
  for (let k = 2; k <= BEHIND; k++) add(-k);
  return order;
}

/** A picture wanted: the photo it belongs to and its medium address. */
export interface Wanted {
  id: number;
  url: string;
}

/** One running load. `done` settles when the picture is loaded and decoded (or rejects). */
export interface Load {
  done: Promise<void>;
  cancel(): void;
}

/** Starts loading and decoding `url`. */
export type Loader = (url: string, priority: "high" | "low") => Load;

/**
 * The page's loader: an `Image` with a fetch priority, decoded ahead of
 * time. A picture that loaded but would not decode still counts as ready
 * if it has a size. The element is kept by the cache, which keeps the
 * decoded picture alive.
 */
export function imageLoader(keep: (url: string, image: HTMLImageElement) => void): Loader {
  return (url, priority) => {
    const image = new Image();
    (image as HTMLImageElement & { fetchPriority?: string }).fetchPriority = priority;
    image.decoding = "async";
    let cancelled = false;
    const done = new Promise<void>((resolve, reject) => {
      image.onload = () => {
        if (cancelled) return;
        image.decode().then(
          () => resolve(),
          () => (image.naturalWidth > 0 ? resolve() : reject(new Error("The picture would not decode."))),
        );
      };
      image.onerror = () => reject(new Error("The picture would not load."));
    }).then(() => keep(url, image));
    image.src = url;
    return {
      done,
      cancel: () => {
        cancelled = true;
        image.onload = null;
        image.onerror = null;
        image.src = "";
      },
    };
  };
}

interface Entry {
  id: number;
  state: "loading" | "ready";
  load: Load | null;
  priority: "high" | "low";
  /** When it was last in a `want` call, for eviction. */
  wanted: number;
  image: HTMLImageElement | null;
}

export class ImageCache {
  /** Moves on whenever what `ready` answers may have changed. */
  version = 0;

  private entries = new Map<string, Entry>();
  /** Address → when it failed; in failing order, so the oldest is first. */
  private failed = new Map<string, number>();
  private wantedList: Wanted[] = [];
  private wantedSet = new Set<string>();
  private wantCount = 0;
  /** Address → how recently it was wanted, for eviction; the first in a call is the most recent. */
  private rank = new Map<string, number>();
  private listeners = new Set<() => void>();
  private readonly loader: Loader;

  constructor(
    loader?: Loader,
    private readonly now: () => number = () => Date.now(),
  ) {
    this.loader =
      loader ??
      imageLoader((url, image) => {
        const e = this.entries.get(url);
        if (e !== undefined) e.image = image;
      });
  }

  /** Whether `url` is loaded and decoded. */
  ready(url: string): boolean {
    return this.entries.get(url)?.state === "ready";
  }

  /** Loads running. */
  get inFlight(): number {
    let n = 0;
    for (const e of this.entries.values()) if (e.state === "loading") n++;
    return n;
  }

  /** The addresses held as ready. */
  held(): string[] {
    return [...this.entries].filter(([, e]) => e.state === "ready").map(([url]) => url);
  }

  /**
   * Replaces the wanted list, most wanted first (the picture on screen,
   * then the preload order). Running loads no longer wanted are cancelled;
   * the first starts at once with high priority, the rest in order while
   * fewer than `MAX_IN_FLIGHT` loads run.
   */
  want(list: readonly Wanted[]): void {
    const seen = new Set<string>();
    this.wantedList = list.filter((w) => w.url !== "" && !seen.has(w.url) && (seen.add(w.url), true));
    this.wantedSet = seen;
    const stamp = ++this.wantCount;
    // Later in the list is less wanted: stamp so the first is the most recent.
    this.rank = new Map(this.wantedList.map((w, i) => [w.url, stamp - i / (this.wantedList.length + 1)]));
    for (const [url, r] of this.rank) {
      const e = this.entries.get(url);
      if (e !== undefined) e.wanted = r;
    }
    let dropped = false;
    for (const [url, e] of this.entries) {
      if (e.state === "loading" && !seen.has(url)) {
        e.load?.cancel();
        this.entries.delete(url);
        dropped = true;
      }
    }
    if (dropped) this.changed();
    this.pump();
  }

  /** Drops the pictures of these photos (their files changed). */
  dropPhotos(ids: readonly number[]): void {
    if (ids.length === 0) return;
    const gone = new Set(ids);
    let dropped = false;
    for (const [url, e] of this.entries) {
      if (!gone.has(e.id)) continue;
      e.load?.cancel();
      this.entries.delete(url);
      dropped = true;
    }
    if (dropped) this.changed();
  }

  /** Forgets everything (the album closed). */
  clear(): void {
    for (const e of this.entries.values()) e.load?.cancel();
    this.entries = new Map();
    this.failed = new Map();
    this.wantedList = [];
    this.wantedSet = new Set();
    this.rank = new Map();
    this.changed();
  }

  subscribe(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  private blocked(url: string): boolean {
    const at = this.failed.get(url);
    if (at === undefined) return false;
    if (this.now() - at < FAILED_FOR_MS) return true;
    this.failed.delete(url);
    return false;
  }

  private pump(): void {
    const [first, ...rest] = this.wantedList;
    if (first === undefined) return;
    if (!this.entries.has(first.url) && !this.blocked(first.url)) this.start(first, "high");
    for (const w of rest) {
      if (this.backgroundRunning() >= MAX_IN_FLIGHT) break;
      if (!this.entries.has(w.url) && !this.blocked(w.url)) this.start(w, "low");
    }
  }

  /** Loads running for anything but the picture on screen, which does not take a place. */
  private backgroundRunning(): number {
    const onScreen = this.wantedList[0]?.url;
    let n = 0;
    for (const [url, e] of this.entries) if (e.state === "loading" && url !== onScreen) n++;
    return n;
  }

  private start(w: Wanted, priority: "high" | "low"): void {
    const entry: Entry = { id: w.id, state: "loading", load: null, priority, wanted: this.rank.get(w.url) ?? this.wantCount, image: null };
    this.entries.set(w.url, entry);
    const load = this.loader(w.url, priority);
    entry.load = load;
    load.done.then(
      () => {
        if (this.entries.get(w.url) !== entry) return;
        entry.state = "ready";
        entry.load = null;
        this.evict();
        this.changed();
        this.pump();
      },
      () => {
        if (this.entries.get(w.url) !== entry) return;
        this.entries.delete(w.url);
        this.failed.delete(w.url);
        this.failed.set(w.url, this.now());
        while (this.failed.size > FAILED_KEPT) this.failed.delete(this.failed.keys().next().value!);
        this.pump();
      },
    );
  }

  /** Ready pictures beyond `CAPACITY` go, least recently wanted first; a wanted picture never does. */
  private evict(): void {
    const ready = [...this.entries].filter(([, e]) => e.state === "ready");
    let over = ready.length - CAPACITY;
    if (over <= 0) return;
    ready.sort(([, a], [, b]) => a.wanted - b.wanted);
    for (const [url] of ready) {
      if (over <= 0) break;
      if (this.wantedSet.has(url)) continue;
      this.entries.delete(url);
      over--;
    }
  }

  private changed(): void {
    this.version++;
    for (const l of this.listeners) l();
  }
}

let cache: ImageCache | null = null;

/** The page's one image cache. */
export function imageCache(): ImageCache {
  cache ??= new ImageCache();
  return cache;
}

/** Puts in a fresh cache, optionally with another loader. For tests. */
export function resetImageCache(loader?: Loader): ImageCache {
  cache?.clear();
  cache = new ImageCache(loader);
  return cache;
}
