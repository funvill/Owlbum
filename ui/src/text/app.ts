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
  /** Owlbum as the program that opens pictures (issue 184). */
  openPictures: "Open pictures with Owlbum",
  openPicturesNote:
    "Offers Owlbum for every kind of picture it can show. Windows then opens Default apps, where you choose Owlbum to make it the default.",
  openPicturesConfirm: "Choose Owlbum in Default apps to finish.",
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
  /** Where the thumbnails sit (issue 185). */
  view: "View",
  gridPlace: "Thumbnails",
  gridLeft: "At the left",
  gridBottom: "Along the bottom, under the preview",
  maps: "Maps",
  mapsOnline: "Load maps from the internet",
  mapsNote: "Map pictures come from OpenStreetMap. Your photos and their locations are never sent.",
  tagging: "Photo tagging",
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
  includes: "It includes software from other projects, each under its own licence: see Third-party notices.",
  repository: "Owlbum on GitHub",
  feedback: "Give feedback",
  checkForUpdates: "Check for updates",
  licence: "Licence",
  notices: "Third-party notices",
  ok: "OK",
  loading: "…",
};

/** Settings → Photo tagging (docs/07-tagging.md, "Settings → Photo tagging"). */
export const taggingSettingsText = {
  intro:
    "Runs on this computer. The models are downloaded from Hugging Face only when you ask, and nothing else leaves the computer.",
  loading: "Loading photo tagging…",
  parts: {
    describe: {
      title: "Descriptions and objects",
      note: "A sentence about each photo, and the things in it.",
    },
    keywords: {
      title: "Keywords and search by meaning",
      note: "Keywords for each photo, and search that finds photos by what you mean.",
    },
  },
  off: "Off",
  sizes: { smaller: "smaller", larger: "larger" },
  details: (bytes: string, speed: string, licence: string) => `${bytes} · ${speed} · ${licence}`,
  lessThanASecond: "less than a second a photo",
  aboutSeconds: (n: number) => `about ${n} s a photo`,
  download: "Download",
  continueDownload: "Continue download",
  downloadAgain: "Not working · Download again",
  busyDownloading: "Another model is downloading. Wait for it, or cancel it.",
  cancel: "Cancel",
  remove: "Remove",
  checking: "Checking…",
  downloading: (label: string) => `Downloading ${label}`,
  percent: (n: number) => `${n}%`,
  confirm: (label: string, size: string) => `Download ${label}, ${size}, from Hugging Face?`,
};

/** Updates (docs/08-desktop-and-release.md, "Messages" and "What the first start says"). */
export const updateText = {
  title: "Owlbum",
  notice:
    "Owlbum checks GitHub for a new version once a day. Nothing about you or your photos is sent. You can turn this off in Help or Settings.",
  turnOff: "Turn off",
  ok: "OK",
  found: (latest: string, current: string) => `Owlbum ${latest} is available (you have ${current}).`,
  download: "Download",
  releaseNotes: "Release notes",
  later: "Later",
  upToDate: (current: string) => `You have the newest version of Owlbum (${current}).`,
  checkFailed: (reason: string) => `Could not check for updates: ${reason}`,
  devBuild: "Updates only work in the installed Owlbum.",
  notSetUp: "This copy of Owlbum cannot update itself. Download new versions from the release page.",
  openReleasePage: "Open the release page",
  close: "Close",
  downloadFailed: (reason: string) => `The update did not download: ${reason}`,
  tryAgain: "Try again",
  downloaded: (version: string) => `Owlbum ${version} is ready to install. Owlbum will close and start again.`,
  restartNow: "Restart now",
  cannotInstall: (problem: string) =>
    `Owlbum cannot install this update by itself: ${problem} Download it from the release page and install it from there.`,
  installFailed: (reason: string) => `The update did not install: ${reason}`,
  checkNow: "Check now",
};

/** What's new (docs/08-desktop-and-release.md, "What's new"). */
export const whatsNewText = {
  title: (version: string) => `What's new in Owlbum ${version}`,
  nothing: "Nothing new to show for this version.",
  ok: "OK",
};

/** The fatal error dialog drawn by the page (docs/08, "The fatal error dialog"). */
export const fatalText = {
  copyDetails: "Copy details",
  copied: "The details are on the clipboard.",
  openLogFolder: "Open the log folder",
  restart: "Restart",
  close: "Close",
};

/**
 * The More menu beside the Settings gear (docs/08-desktop-and-release.md,
 * "The More menu"; owner's feedback, issue 197). It holds what the menu bar
 * and the Tidy button did.
 */
export const moreText = {
  open: "More",
  openFolder: "Open folder…",
  closeFolder: "Close folder",
  rescan: "Rescan",
  tidy: "Tidy…",
  thumbnails: "Thumbnails",
  details: "Details panel",
  thumbnailsBottom: "Thumbnails at the bottom",
  fullScreen: "Full screen",
  dataFolder: "Open the data folder",
  shortcuts: "Keyboard shortcuts",
  whatsNew: "What's new",
  feedback: "Give feedback",
  about: "About Owlbum",
  reload: "Reload",
  developerTools: "Developer tools",
  quit: "Quit",
  noAlbum: "Open a folder first",
  /** The update item follows the updater's state (docs/08, "States"). */
  update: {
    check: "Check for updates…",
    checking: "Checking for updates…",
    download: (version: string) => `Download update ${version}…`,
    downloading: (percent: number) => `Downloading update… ${percent} %`,
    install: (version: string) => `Install update ${version}…`,
    installing: "Installing update…",
    busy: "Wait for the updater to finish",
  },
};
