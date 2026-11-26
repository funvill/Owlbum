// Text shown by the details feature (src/features/details).
// docs/04-browsing.md, "The details panel".

export const detailsText = {
  countrySearch: "Search for a country…",
  countryMatches: "Countries",

  // The panel
  region: "Details",
  noPhoto: "No photo selected",
  close: "Close details",
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
  },

  // Fact rows
  taken: "Taken",
  size: "Size",
  length: "Length",
  resolution: "Resolution",
  shape: "Shape",
  format: "Format",
  folder: "Folder",
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

  // Typed info, shown read-only until #24 makes them fields
  people: "People",
  noPeople: "No people named",
  dateTaken: "Date and time taken",
  description: "Description",
  notes: "Notes",
  empty: "None",
};

export type SectionId = keyof typeof detailsText.sections;
