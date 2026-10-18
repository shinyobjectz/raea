// JS-only cost: the same board through renderToString, no DOM writes (compare with client.tsx).
import { renderToString } from "react-dom/server"
import { Board, type Task } from "./Board"
const html = renderToString(<Board tasks={(window as any).__TASKS__ as Task[]} />)
document.getElementById("root")!.setAttribute("data-len", String(html.length))
