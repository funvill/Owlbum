// Whether the About box is open. F1 and Help > About Owlbum open it
// through `app_menu` (docs/08-desktop-and-release.md, "About box").

import { create } from "zustand";

export const useAboutOpen = create<{ open: boolean }>(() => ({ open: false }));

export function openAbout(): void {
  useAboutOpen.setState({ open: true });
}

export function closeAbout(): void {
  useAboutOpen.setState({ open: false });
}
