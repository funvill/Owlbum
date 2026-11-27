import { settingsText as t } from "../../text/app";

/**
 * The Photo tagging section of Settings: the model choices, downloads and
 * removal (`tagging.describe`, `tagging.keywords`), owned entirely by
 * docs/07-tagging.md. The Settings dialog only gives it a place.
 *
 * SLOT for issue 57 (batch I, tagging settings): replace this stub's body with
 * the tagging controls. The dialog renders it under the "Photo tagging"
 * heading and needs no other change.
 */
export function TaggingSettings() {
  return <p className="ow-settings__note">{t.taggingLater}</p>;
}
