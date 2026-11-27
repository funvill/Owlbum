// A video on the preview's or the viewer's stage (docs/04-browsing.md,
// "Video"): the poster frame with a round play button; a click replaces it
// with the system's player, which starts with sound. If the player cannot
// play the file, the poster comes back with a sentence and Show in folder.

import { type ReactNode, useEffect, useState } from "react";
import { videoUrl } from "../../api/media";
import { Icon } from "../../icons/Icon";
import { previewText as t } from "../../text/preview";
import { revealPhoto } from "../files/actions";
import { setVideoElement, stopVideo, useVideoStore, type VideoPlace, playVideo, videoKey } from "./video";
import "./VideoStage.css";

export interface VideoStageProps {
  photoId: number;
  place: VideoPlace;
  /** The poster: the stage that shows the video's thumbnail. */
  children: ReactNode;
}

export function VideoStage({ photoId, place, children }: VideoStageProps) {
  const key = videoKey(place, photoId);
  const playing = useVideoStore((s) => s.playing === key);
  const [failed, setFailed] = useState(false);

  // Moving to another photo or leaving the page lets the player go.
  useEffect(
    () => () => {
      if (useVideoStore.getState().playing === key) stopVideo();
    },
    [key],
  );

  if (playing && !failed) {
    return (
      <div className="ow-video">
        <video
          ref={setVideoElement}
          className="ow-video__player"
          src={videoUrl(photoId)}
          controls
          autoPlay
          onError={() => {
            setFailed(true);
            stopVideo();
          }}
        />
      </div>
    );
  }

  return (
    <div className="ow-video">
      {children}
      {failed ? (
        <div className="ow-video__failed" role="status">
          <p>{t.cannotPlay}</p>
          <button type="button" className="ow-video__reveal" onClick={() => void revealPhoto(photoId)}>
            {t.showInFolder}
          </button>
        </div>
      ) : (
        <button
          type="button"
          className="ow-video__play"
          aria-label={t.play}
          onClick={() => playVideo(place, photoId)}
          onDoubleClick={(e) => e.stopPropagation()}
        >
          <Icon name="play" size="lg" />
        </button>
      )}
    </div>
  );
}

/** A video's stage has a play button over its poster; a photo's is the zoom stage alone. */
export function StageFor({
  kind,
  photoId,
  place,
  children,
}: {
  kind: string;
  photoId: number;
  place: VideoPlace;
  children: ReactNode;
}) {
  if (kind !== "video") return children;
  return (
    <VideoStage key={photoId} photoId={photoId} place={place}>
      {children}
    </VideoStage>
  );
}
