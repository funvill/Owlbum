// Text shown by the details feature (src/features/details).
// docs/04-browsing.md, "The details panel".

export const detailsText = {
  countrySearch: "Search for a country…",
  countryMatches: "Countries",

  // The panel
  region: "Details",
  noPhoto: "No photo selected",
  copyAll: "Copy",
  copyAllTooltip: "Copy all of these details as plain text",
  copiedAll: "Details copied",
  viewOnlyNote: "This album is view-only, so nothing here can be changed.",
  couldNotRead: "The details of this photo could not be read.",

  // Section titles, by id
  sections: {
    photo: "Photo",
    whatsinit: "What's in it",
    camera: "Camera",
    histogram: "Histogram",
    place: "Place",
    people: "People",
    date: "Date and time",
    notes: "Description and notes",
    exif: "EXIF",
  },

  // Fact rows
  taken: "Taken",
  size: "Size",
  length: "Length",
  resolution: "Resolution",
  shape: "Shape",
  format: "Format",
  folder: "Folder",
  path: "Path",
  copyPath: "Copy path",
  unknown: "Unknown",
  changed: "changed",
  changedTooltip: "The date was changed here; the camera's is kept in the photo",
  square: "Square",
  landscape: "Landscape",
  portrait: "Portrait",
  months: ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"],

  // Camera rows
  camera: "Camera",
  lens: "Lens",
  focalLength: "Focal length",
  aperture: "Aperture",
  shutter: "Shutter",
  iso: "ISO",
  exposure: "Exposure",
  flash: "Flash",
  flashFired: "Fired",
  flashNotFired: "Did not fire",
  in35mm: (mm: string) => ` (${mm} mm in 35 mm terms)`,
  noCamera: "No camera details in this file",

  // EXIF (owner's feedback, issue 167)
  noExif: "No EXIF data in this file",
  exifFailed: "The EXIF data could not be read.",
  scoreLabel: "Photo score",

  // Histogram
  histogramLabel: "Histogram of brightness and the red, green and blue channels",
  shadowsClipped: (pct: string) => `Shadows clipped ${pct} %`,
  highlightsClipped: (pct: string) => `Highlights clipped ${pct} %`,
  histogramFailed: "The histogram could not be drawn.",

  // Place rows
  country: "Country",
  countryLooking: "…",
  coordinates: "Coordinates",
  placeName: "Place name",

  // Typed info (docs/04-browsing.md, "How typed fields save")
  saved: "Saved",
  location: "Location",
  latitude: "Latitude",
  longitude: "Longitude",
  people: "People",
  addName: "Add a name, press Enter",
  suggestions: "Names used in this album",
  dateTaken: "Date and time taken",
  datePlaceholder: "YYYY-MM-DD HH:MM",
  useCameraDate: "Use the camera's date",
  clearDate: "Clear the date",
  description: "Description",
  notes: "Notes",
  invalidDate: "That is not a valid date and time. Use YYYY-MM-DD HH:MM.",
  latNumber: "Latitude must be a number.",
  lonNumber: "Longitude must be a number.",
  latRange: "Latitude must be between -90 and 90.",
  lonRange: "Longitude must be between -180 and 180.",
  bothNeeded: "Enter both latitude and longitude.",
  nameNeedsPlace: "Add a latitude and longitude for this place, or clear the name.",
};

export type SectionId = keyof typeof detailsText.sections;
