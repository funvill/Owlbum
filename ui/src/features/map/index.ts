// The Map view (docs/04-browsing.md, "Map view"): the result's located
// photos as markers on a map, in place of the thumbnails.
export { resetMapFocus, showOnMap, useMapFocus } from "./focus";
export { MapView, resetMapPlace, type MapViewProps } from "./MapView";
export { diffPoints, samePoint, type PointChange } from "./points";
export { PickPlaceButton, PlacePicker, pickTargets, type PlacePickerProps } from "./PlacePicker";
