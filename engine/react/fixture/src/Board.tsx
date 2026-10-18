import { useState } from "react"

export type Task = { id: number; title: string; status: string; assignee: string; due: string; tags: string[] }
export const COLUMNS = ["Todo", "Doing", "Review", "Done"]

function Card({ task, onAdvance }: { task: Task; onAdvance?: (id: number) => void }) {
  return (
    <div className="card" id={`task-${task.id}`}>
      <div className="title">{task.title}</div>
      <div className="meta">
        <span className="who">{task.assignee}</span>
        <span className="due">{task.due}</span>
      </div>
      <div className="tags">
        {task.tags.map((t) => (
          <span key={t} className={`tag tag-${t}`}>
            {t}
          </span>
        ))}
      </div>
      {task.status !== "Done" && (
        <button className="advance" data-advance={task.id} onClick={() => onAdvance?.(task.id)}>
          Move on →
        </button>
      )}
    </div>
  )
}

export function Board({ tasks }: { tasks: Task[] }) {
  return (
    <div className="board">
      <h1>Launch plan</h1>
      <div className="columns">
        {COLUMNS.map((col) => {
          const items = tasks.filter((t) => t.status === col)
          return (
            <section className="column" key={col}>
              <h2>
                {col} <span className="count">{items.length}</span>
              </h2>
              {items.map((t) => (
                <Card key={t.id} task={t} onAdvance={(window as any).__advance} />
              ))}
            </section>
          )
        })}
      </div>
    </div>
  )
}

// The interactive version owns the task list as state.
export function LiveBoard({ initial }: { initial: Task[] }) {
  const [tasks, setTasks] = useState(initial)
  ;(globalThis as any).window ??= globalThis
  ;(window as any).__advance = (id: number) =>
    setTasks((ts) => ts.map((t) => (t.id === id ? { ...t, status: COLUMNS[COLUMNS.indexOf(t.status) + 1] ?? t.status } : t)))
  return <Board tasks={tasks} />
}
