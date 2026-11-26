import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { setApi } from "./api";
import { createTauriApi } from "./api/tauri";
import { App } from "./App";
import "./main.css";

setApi(createTauriApi());

const root = document.getElementById("root");
if (root === null) throw new Error("index.html has no #root element");
createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
