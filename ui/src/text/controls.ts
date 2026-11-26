// Text shown by the base controls (src/controls).

export const controlsText = {
  close: "Close",
  dismiss: "Dismiss",
  clearSearch: "Clear search",
  removeChip: (label: string) => `Remove ${label}`,
  count: (label: string, count: number) => `${label} ×${count}`,
  tooltipWithShortcut: (label: string, shortcut: string) => `${label} (${shortcut})`,
};
