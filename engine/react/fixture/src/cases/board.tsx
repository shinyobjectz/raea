// Static render: createRoot + flushSync, flexbox columns, mapped lists with keys.
import { createRoot } from "react-dom/client"
import { flushSync } from "react-dom"
import { Board, type Task } from "../Board"

const S = ["Todo", "Doing", "Review", "Done"], P = ["Ana", "Ben", "Chi", "Dev", "Eli"], T = ["bug", "design", "infra"]
const W = ["Ship onboarding", "Fix sync bug", "Draft brief", "Review copy", "Set up backups"]
const tasks: Task[] = Array.from({ length: 14 }, (_, i) => ({
  id: i, title: `Task ${i}: ${W[i % 5]}`, status: S[(i * 7 + Math.floor(i / 3)) % 4], assignee: P[i % 5],
  due: `Oct ${1 + (i % 28)}`, tags: T.filter((_, k) => (i >> k) & 1),
}))
const root = createRoot(document.getElementById("app")!)
flushSync(() => root.render(<Board tasks={tasks} />))
