// Real react-dom against the page's DOM (in raea: Boa + blitz-dom), rendered synchronously.
import { createRoot } from "react-dom/client"
import { flushSync } from "react-dom"
import { Board, type Task } from "./Board"

const t0 = performance.now()
const tasks: Task[] = (window as any).__TASKS__
const root = createRoot(document.getElementById("root")!)
flushSync(() => root.render(<Board tasks={tasks} />))
document.body.setAttribute("data-react-ms", String(Math.round(performance.now() - t0)))
