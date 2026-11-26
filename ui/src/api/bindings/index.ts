// Generated from the Rust command registry (src-tauri/src/bindings.rs). Do not edit;
// run scripts/bindings.sh after changing a command, an event or a type.

import type { AlbumCommands, AlbumEvents } from "./album";
import type { AppCommands, AppEvents } from "./app";
import type { EditCommands, EditEvents } from "./edit";
import type { FileCommands, FileEvents } from "./file";
import type { PhotoCommands, PhotoEvents } from "./photo";
import type { QueryCommands, QueryEvents } from "./query";
import type { SearchCommands, SearchEvents } from "./search";
import type { TaggingCommands, TaggingEvents } from "./tagging";

/** Every command: name to `{ args, result }`. */
export type Commands = AlbumCommands &
  AppCommands &
  EditCommands &
  FileCommands &
  PhotoCommands &
  QueryCommands &
  SearchCommands &
  TaggingCommands;

/** Every event: name to payload. */
export type Events = AlbumEvents &
  AppEvents &
  EditEvents &
  FileEvents &
  PhotoEvents &
  QueryEvents &
  SearchEvents &
  TaggingEvents;
