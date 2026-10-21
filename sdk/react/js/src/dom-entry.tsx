// The same App through react-dom, in a page the engine renders today (the comparison for the mirror).
import { createRoot } from "react-dom/client";
import { flushSync } from "react-dom";
import { App } from "./app";

const root = createRoot(document.getElementById("root")!);
flushSync(() => root.render(<App />));
