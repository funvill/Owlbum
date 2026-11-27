// Which video is playing, if any (docs/04-browsing.md, "Video").
//
// At most one player exists: the preview's or the viewer's. Nothing plays
// by itself; moving to another photo, closing the viewer, opening the
// viewer from the preview and closing the album all let the player go.

import { create } from "zustand";

/** Where a player is: the preview pane or the full-screen viewer. */
export type VideoPlace = "preview" | "viewer";

interface VideoState {
  /** `${place}:${photoId}` of the player that is open, or null. */
  playing: string | null;
}

export const useVideoStore = create<VideoState>(() => ({ playing: null }));

/** The open player's element, for the viewer's play-and-pause key. */
let element: HTMLVideoElement | null = null;

export const videoKey = (place: VideoPlace, photoId: number) => `${place}:${photoId}`;

/** Replaces the poster with a player that starts at once. */
export function playVideo(place: VideoPlace, photoId: number): void {
  useVideoStore.setState({ playing: videoKey(place, photoId) });
}

/** Lets the player go, wherever it is. */
export function stopVideo(): void {
  element = null;
  if (useVideoStore.getState().playing !== null) useVideoStore.setState({ playing: null });
}

/** The player's element, as it mounts and unmounts. */
export function setVideoElement(el: HTMLVideoElement | null): void {
  element = el;
}

/** Plays or pauses the video at `place`; opens its player when none is open. */
export function toggleVideo(place: VideoPlace, photoId: number): void {
  if (useVideoStore.getState().playing === videoKey(place, photoId) && element !== null) {
    if (element.paused) void element.play().catch(() => {});
    else element.pause();
    return;
  }
  playVideo(place, photoId);
}
