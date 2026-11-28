// Generated from the Rust command registry (src-tauri/src/bindings.rs). Do not edit;
// run scripts/bindings.sh after changing a command, an event or a type.

import type { DownloadProgress } from "./DownloadProgress";
import type { ModelsState } from "./ModelsState";
import type { PhotoTags } from "./PhotoTags";
import type { RetryCleared } from "./RetryCleared";
import type { TaggingChoice } from "./TaggingChoice";
import type { TaggingProgress } from "./TaggingProgress";

export type TaggingCommands = {
  tagging_download: { args: { model: string }; result: null };
  tagging_download_cancel: { args: Record<string, never>; result: null };
  tagging_focus: { args: { ids: Array<number> }; result: null };
  tagging_models: { args: Record<string, never>; result: ModelsState };
  tagging_pause: { args: { paused: boolean }; result: null };
  tagging_regenerate: { args: { id: number }; result: PhotoTags };
  tagging_remove: { args: { model: string }; result: null };
  tagging_restart: { args: Record<string, never>; result: null };
  tagging_retry_failed: { args: Record<string, never>; result: RetryCleared };
  tagging_set: { args: { describe: string | null; keywords: string | null }; result: TaggingChoice };
  tagging_status: { args: Record<string, never>; result: TaggingProgress };
};

export type TaggingEvents = {
  download_progress: DownloadProgress;
  tagging_progress: TaggingProgress;
};
