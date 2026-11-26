// The key table and key matching (docs/08-desktop-and-release.md,
// "The key table"). Key handlers, the shortcut overlay and tooltips use
// these; nothing else defines a key.
export {
  KEYMAP,
  KEY_IDS,
  KeymapError,
  handlerGaps,
  parseKey,
  parseKeymap,
  primaryKey,
  row,
  rowsFor,
} from "./table";
export type { KeyId, KeyRow, KeySpec, Place } from "./table";
export { focusOf, isRelease, keyMatches, matchKey } from "./match";
export type { Focus, KeyLike, KeyMatch, MatchContext } from "./match";
