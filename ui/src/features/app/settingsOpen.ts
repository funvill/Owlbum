// Whether the Settings dialog is open. The top bar's gear and the shortcut
// overlay's Settings button both open it (docs/08-desktop-and-release.md).

import { create } from "zustand";

export const useSettingsOpen = create<{ open: boolean }>(() => ({ open: false }));

export function openSettings(): void {
  useSettingsOpen.setState({ open: true });
}

export function closeSettings(): void {
  useSettingsOpen.setState({ open: false });
}
