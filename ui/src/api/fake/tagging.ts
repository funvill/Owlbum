import type { TaggingCommands } from "../bindings/tagging";
import type { ModelInfo } from "../bindings/ModelInfo";
import type { ModelsState } from "../bindings/ModelsState";
import type { TaggingProgress } from "../bindings/TaggingProgress";
import type { AreaHandlers } from "./types";
import type { AppError } from "../bindings/AppError";

/** One catalogue model as Settings lists it, not downloaded. */
function model(
  id: string,
  part: ModelInfo["part"],
  size: ModelInfo["size"],
  label: string,
  bytes: number,
  speed: number,
): ModelInfo {
  return {
    id,
    part,
    size,
    label,
    bytes,
    licence: "MIT",
    seconds_per_photo: speed,
    state: "absent",
    bytes_present: 0,
  };
}

/** The four catalogue models, none downloaded. */
export const fakeModels: readonly ModelInfo[] = [
  model(
    "florence2-base-q8",
    "describe",
    "smaller",
    "Florence-2 base",
    277_470_179,
    6,
  ),
  model(
    "florence2-large-q8",
    "describe",
    "larger",
    "Florence-2 large",
    832_001_774,
    20,
  ),
  model("clip-base-q8", "keywords", "smaller", "CLIP base", 155_851_446, 0.1),
  model("clip-large-q8", "keywords", "larger", "CLIP large", 434_215_233, 0.5),
];

/** Tagging with no model chosen: nothing to count. */
export const idleProgress: TaggingProgress = {
  total: 0,
  done: 0,
  failed: 0,
  paused: false,
  running: false,
  not_working: [],
  problem: null,
};

/** The models state with nothing downloaded and both parts Off. */
export function fakeModelsState(): ModelsState {
  return {
    models: fakeModels.map((m) => ({ ...m })),
    settings: { describe: null, keywords: null },
    not_working: [],
    downloading: null,
    error: null,
  };
}

/** The fake backend's `tagging_` commands: no model is installed, so tagging is off. */
export const taggingHandlers: AreaHandlers<TaggingCommands> = {
  tagging_models: () => fakeModelsState(),
  tagging_download: () => null,
  tagging_download_cancel: () => null,
  tagging_remove: () => null,
  tagging_set: ({ describe, keywords }) => ({ describe, keywords }),
  tagging_status: () => ({ ...idleProgress }),
  tagging_pause: () => null,
  tagging_restart: () => null,
  tagging_retry_failed: () => ({ cleared: 0 }),
  tagging_focus: () => null,
  tagging_regenerate: () => {
    const e: AppError = {
      code: "no_describe_model",
      message: "There is no working descriptions model. Choose one in Settings.",
    };
    throw e;
  },
};
