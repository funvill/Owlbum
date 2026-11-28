import { api } from "../../api";
import { chooseLanguage, showLanguage, systemLanguages } from "../../text/i18n/languages";

/**
 * The interface language at start (docs/09-visual-design.md, "Languages";
 * issue 76): the `language` setting when it names an available language,
 * else the system's language when a translation of it exists, else
 * English. A change in Settings is shown from the next start, since text
 * already drawn is not drawn again.
 */
export async function startLanguage(): Promise<string> {
  let chosen: string | null = null;
  try {
    chosen = (await api().invoke("app_settings_get")).language;
  } catch {
    // Unreadable settings: the system's language decides.
  }
  const tag = chooseLanguage(chosen, systemLanguages());
  await showLanguage(tag);
  return tag;
}
