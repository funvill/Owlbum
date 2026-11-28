// Whether the Settings dialog is open. The top bar's gear and the shortcut
// overlay's Settings button both open it (docs/08-desktop-and-release.md).

import { create } from "zustand";

/** A section the dialog scrolls to when it opens. */
export type SettingsSection = "tagging";

export const useSettingsOpen = create<{ open: boolean; section: SettingsSection | null }>(() => ({
  open: false,
  section: null,
}));

/** Opens Settings, at `section` when one is given (the tagging notes' Settings link). */
export function openSettings(section: SettingsSection | null = null): void {
  useSettingsOpen.setState({ open: true, section });
}

export function closeSettings(): void {
  useSettingsOpen.setState({ open: false, section: null });
}
