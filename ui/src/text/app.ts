// Text shown by the app feature (src/features/app).

export const appText = {
  name: "Owlbum",
  version: (version: string) => `Version ${version}`,
  loading: "Starting…",
};

/** The Settings dialog (docs/08-desktop-and-release.md, "Settings dialog"). */
export const settingsText = {
  title: "Settings",
  open: "Settings",
  close: "Close",
  general: "General",
  reopenLast: "Reopen the last album at start",
  cache: "Thumbnail cache",
  cacheLimit: "Cache limit in GB",
  cacheUses: (used: string) => `Uses ${used} of`,
  cacheUnit: "GB",
  cacheUnknown: "…",
  cacheNote: "Thumbnails are made again when needed. Clearing loses nothing.",
  cacheRange: "Use a whole number from 1 to 1000. The limit was put back.",
  clearCache: "Clear cache",
  clearConfirm: (size: string) => `Clear ${size} of thumbnails? They are made again as you browse.`,
  clearCancel: "Cancel",
  clear: "Clear",
  cleared: (size: string) => `Cleared ${size} of thumbnails.`,
  editTarget: "When I edit a photo",
  editAlbum: "Keep the edit in the album",
  editAlbumNote: "Done is the save. The photo file is never touched.",
  editFile: "Save edits into the photo file",
  editFileNote: "An edit waits as “not saved” until Save writes it into the file.",
  maps: "Maps",
  mapsOnline: "Load maps from the internet",
  mapsNote: "Map pictures come from OpenStreetMap. Your photos and their locations are never sent.",
  tagging: "Photo tagging",
  taggingLater: "Photo tagging is not available in this version yet.",
  updates: "Updates",
  updatesAuto: "Check for a new version once a day",
  versionLine: (version: string, checked: string) => `Owlbum ${version} · last checked ${checked}`,
  never: "never",
  today: (time: string) => `today at ${time}`,
  onDay: (day: string, time: string) => `${day} at ${time}`,
  couldNotKeep: (reason: string) => `Couldn't keep the setting: ${reason}`,
};

/** The About box (docs/08-desktop-and-release.md, "About box"). */
export const aboutText = {
  title: "About Owlbum",
  name: (version: string) => `Owlbum ${version}`,
  versionLine: (version: string, platform: string, arch: string) => `Version ${version} (${platform} ${arch})`,
  description:
    "Look through folders of photos, star the best ones, find things by what is in them, and drag photos into other programs.",
  network:
    "Everything stays on this computer. Owlbum uses the network only for OpenStreetMap map tiles, and only after you choose Load maps on the Map tab; to download photo-tagging models from Hugging Face when you ask for one in Settings; and to check GitHub for a new version once a day, unless you turn that off in Help.",
  dataFolder: "Its thumbnails, settings and models are kept in:",
  credits: "Built from the open-source project funvill/Owlbum on GitHub, under the MIT License.",
  repository: "Owlbum on GitHub",
  feedback: "Give feedback",
  ok: "OK",
  loading: "…",
};
