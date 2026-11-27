// leaflet.markercluster extends the global `L`, so Leaflet is made global
// before it loads. Imported between the two.

import L from "leaflet";

(globalThis as { L?: typeof L }).L = L;
