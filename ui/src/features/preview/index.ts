// The preview pane and the zoom component it shares with the viewer
// (docs/04-browsing.md, "Preview pane").
export { Preview, type PreviewProps } from "./Preview";
export { Caption, type CaptionProps } from "./Caption";
export {
  ZOOM_KEY_IDS,
  ZoomStage,
  zoomKeyHandlers,
  type StagePictures,
  type ZoomHandle,
  type ZoomKeyId,
  type ZoomStageProps,
} from "./ZoomStage";
export * from "./zoom";
