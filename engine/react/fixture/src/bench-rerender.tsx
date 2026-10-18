// Reconciliation cost: mount the board, then re-render it 40 times with tasks moving between
// columns (what a per-frame React composition does), all synchronously.
import { createRoot } from "react-dom/client"
import { flushSync } from "react-dom"
import { Board, type Task } from "./Board"

const S = ["Todo", "Doing", "Review", "Done"]
const tasks: Task[] = (window as any).__TASKS__
const root = createRoot(document.getElementById("root")!)
for (let frame = 0; frame < 41; frame++) {
  const moved = tasks.map((t) => (t.id % 7 === frame % 7 ? { ...t, status: S[(S.indexOf(t.status) + 1) % 4] } : t))
  flushSync(() => root.render(<Board tasks={moved} />))
}
