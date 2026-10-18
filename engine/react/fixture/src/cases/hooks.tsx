// Effects and refs: useLayoutEffect measures a box and sizes a bar to it; useEffect sets state after
// mount; useMemo/useReducer/context. Everything must settle before the first frame.
import { createContext, useContext, useEffect, useLayoutEffect, useMemo, useReducer, useRef, useState } from "react"
import { createRoot } from "react-dom/client"
import { flushSync } from "react-dom"

const Theme = createContext({ accent: "#d9480f" })

function Measured() {
  const box = useRef<HTMLDivElement>(null)
  const [w, setW] = useState(0)
  useLayoutEffect(() => setW(Math.round(box.current!.getBoundingClientRect().width)), [])
  const { accent } = useContext(Theme)
  return (
    <div className="panel">
      <div ref={box} className="label">Measured with useLayoutEffect</div>
      <div className="bar" style={{ width: w, background: accent }} />
      <div className="num">{w}px</div>
    </div>
  )
}

function Loaded() {
  const [status, setStatus] = useState("loading")
  useEffect(() => setStatus("ready after useEffect"), [])
  return <div className={`panel ${status === "loading" ? "wait" : "ok"}`}>{status}</div>
}

function Counter() {
  const [n, add] = useReducer((n: number, k: number) => n + k, 0)
  useEffect(() => { add(3); add(4) }, [])
  const squares = useMemo(() => Array.from({ length: n }, (_, i) => i), [n])
  return (
    <div className="panel">
      <div className="label">useReducer: {n}</div>
      <div className="dots">{squares.map((i) => <span key={i} className="dot" style={{ opacity: 0.3 + i / 10 }} />)}</div>
    </div>
  )
}

const root = createRoot(document.getElementById("app")!)
flushSync(() =>
  root.render(
    <Theme.Provider value={{ accent: "#1c7ed6" }}>
      <Measured />
      <Loaded />
      <Counter />
    </Theme.Provider>,
  ),
)
