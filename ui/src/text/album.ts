// Text shown by the album feature (src/features/album).
// docs/04-browsing.md, "Start screen", "Opening a folder", "Top bar",
// "Album banners", "Reading bar", "Notices" and "Toasts and other dialogs".

/** "1 photo", "2,140 photos": the plural "s" only when the number is not 1. */
function plural(n: number, one: string, many: string): string {
  return n === 1 ? one : many;
}

/** "2,140": thousands separators. */
function count(n: number): string {
  return n.toLocaleString("en-US");
}

export const albumText = {
  // Reading bar
  readingLabel: "Reading photos",
  scanWalking: "Looking for photos…",
  scanReading: (done: number, found: number) => `Reading photos ${done} / ${found}`,
  lessThanAMinute: "less than a minute left",
  minutesLeft: (m: number) => `about ${m} ${plural(m, "minute", "minutes")} left`,
  hoursLeft: (h: number, m: number) =>
    `about ${h} ${plural(h, "hour", "hours")} ${m} ${plural(m, "minute", "minutes")} left`,

  // Start screen
  startHeading: "Open a folder of photos",
  startIntro: "Look through your photos, star the best ones and drag them into other programs. Nothing leaves this computer.",
  openFolder: "Open folder",
  recentHeading: "Recent folders",
  notFound: "Not found — is the drive connected?",
  removeRecent: "Remove from this list",

  // Body
  opening: (name: string) => `Opening ${name}…`,
  lookingForPhotos: "Looking for photos…",
  noPhotos: "No photos in this folder.",
  dropHere: "Drop to open this folder",
  photosRegion: "Photos",
  /** The reading bar's landmark name. */
  readingRegion: "Reading progress",

  // Review screen splitters (docs/04-browsing.md, "Splitters")
  resizeGrid: "Resize thumbnail column",
  resizeDetails: "Resize details panel",

  // Top bar
  albumTooltip: (root: string) => `Album: ${root}`,
  scopeTooltip: (folder: string) => `Showing only: ${folder}`,
  showWholeAlbum: "Show whole album",
  viewOnlyBadge: "View only",
  closeFolder: "Close folder",
  tidy: "Tidy…",
  tidyTooltip: "Remove what Owlbum remembers about photos that are no longer in the album",

  // Tidy (docs/05-organising.md, "Tidy")
  tidyTitle: "Tidy album",
  tidyLooking: "Looking…",
  tidyNothing: "Nothing to tidy: every photo Owlbum knows about is still in the album.",
  tidyScanning: "Owlbum is still reading the album. Tidy when it has finished.",
  tidyExplain:
    "These photos are no longer in the album. Removing them deletes their stars, notes, people and tags for good. No photo file is touched.",
  tidyTopFolder: "(top folder)",
  tidyFolderGone: "This whole folder is gone",
  tidyFolderGoneTooltip:
    "If it is on a drive that is not plugged in, plug it in and let Owlbum read the album again before tidying.",
  tidySelected: (n: number, of: number) => `${n} of ${of} selected`,
  tidyRemove: (n: number) => `Remove ${n} ${plural(n, "entry", "entries")}`,
  tidyRemoved: (n: number) => `Removed ${n} ${plural(n, "entry", "entries")}.`,
  tidyRemovedKept: (n: number, kept: number) =>
    `Removed ${n} ${plural(n, "entry", "entries")}. ${kept} ${plural(kept, "photo was", "photos were")} found again and kept.`,
  close: "Close",

  // Banners
  viewOnlyLocked: (host: string) =>
    `View only: this album is open on ${host}. Stars, notes, edits and tagging are off until it is closed there.`,
  viewOnlyReadOnly: "View only: this folder can't be written to. Stars, notes, edits and tagging are off.",
  viewOnlyInMemory: "View only: this folder can't be written to, so nothing Owlbum learns about these photos is kept.",
  viewOnlyNewer: "This album was last used by a newer Owlbum. It is open view-only.",
  viewOnlyDamaged: "View only: this album's database is damaged.",
  viewOnlyChosenHost: (host: string) => `View only: this album may be open on ${host}.`,
  viewOnlyChosen: "View only: you opened this album view-only.",
  conflict: (file: string) =>
    `Another computer changed this album at the same time. Owlbum is using its own copy and has not merged "${file}".`,
  tryAgain: "Try again",
  restoreBackupEllipsis: "Restore backup…",
  openForChanges: "Open for changes",

  // Notices and the Files not shown dialog
  noticesLabel: "Files not shown",
  unsupportedNotice: (n: number, labels: readonly string[]) =>
    `${count(n)} ${plural(n, "file", "files")} not supported yet (${labels.join(", ")})`,
  unreadableNotice: (n: number) => `${count(n)} ${plural(n, "file", "files")} could not be read`,
  noticeSeparator: " · ",
  notShownTitle: "Files not shown",
  notShownLoading: "Reading the list…",
  notShownEmpty: "Every file is shown.",
  copyList: "Copy list",
  listCopied: "Copied the list",

  // Questions when opening. Their sentences are the error's message, from Rust.
  staleTitle: "Is this album open somewhere else?",
  openAnyway: "Open anyway",
  openViewOnly: "Open view-only",
  damagedTitle: "This album's database is damaged",
  restoreBackup: "Restore backup",
  cancel: "Cancel",

  // Toasts
  couldNotOpen: (name: string, message: string) => `Couldn't open ${name}: ${message}`,
  lastFolderMissing: (name: string) => `The last folder isn't available: ${name}`,
  dropAFolder: "Drop a folder to open it.",
  // docs/03-media-and-scanning.md, "When watching cannot be trusted": said once per session.
  notWatching:
    "Owlbum can't watch this folder for changes. It checks when you come back to the window; use Rescan to check now.",
};
