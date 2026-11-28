import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { setApi } from "./api";
import { createTauriApi } from "./api/tauri";
import { App } from "./App";
import { followPageErrors } from "./features/app/pageLog";
import { followTheme } from "./features/app/theme";
import { startLanguage } from "./features/app/language";
import "@fontsource-variable/inter/wght.css";
import "./styles/tokens.css";
import "./styles/base.css";
import "./main.css";

setApi(createTauriApi());
followPageErrors();
followTheme();
// The language is set before anything is drawn: text is read as it is drawn.
await startLanguage();

const root = document.getElementById("root");
if (root === null) throw new Error("index.html has no #root element");
createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
