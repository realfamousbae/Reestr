import "@fontsource/pt-sans/400.css";
import "@fontsource/pt-sans/700.css";
import "./theme/tokens.css";

import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { App } from "./App";

/** The grid draws on a canvas, which does not re-render when a web font arrives late. */
async function fontsReady(): Promise<void> {
  const timeout = new Promise<void>((resolve) => setTimeout(resolve, 1500));
  const load = Promise.all([
    document.fonts.load('400 15px "PT Sans"', "Аа1"),
    document.fonts.load('700 15px "PT Sans"', "Аа1"),
  ]).then(() => undefined);
  await Promise.race([load, timeout]);
}

const root = document.getElementById("root");
if (root !== null) {
  void fontsReady().then(() => {
    createRoot(root).render(
      <StrictMode>
        <App />
      </StrictMode>,
    );
  });
}
