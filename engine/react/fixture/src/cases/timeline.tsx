// A video-style component: the GSAP timeline is the clock and React re-renders every frame from it.
import { createRoot } from "react-dom/client"
import { flushSync } from "react-dom"

declare const gsap: any
const clamp = (x: number) => Math.max(0, Math.min(1, x))

function Scene({ t }: { t: number }) {
  const p = clamp(t / 1.5)
  const items = ["Plan", "Shoot", "Edit", "Post"]
  return (
    <div className="scene">
      <h1 style={{ opacity: clamp(t / 0.4), transform: `translateY(${(1 - clamp(t / 0.4)) * 20}px)` }}>Weekly progress</h1>
      <div className="track"><div className="fill" style={{ width: `${Math.round(p * 100)}%` }} /></div>
      <div className="pct">{Math.round(p * 100)}%</div>
      <ul>
        {items.map((s, i) => (
          <li key={s} className={t > 0.4 + i * 0.3 ? "done" : ""} style={{ opacity: clamp((t - i * 0.2) / 0.3) }}>{s}</li>
        ))}
      </ul>
    </div>
  )
}

const root = createRoot(document.getElementById("app")!)
const draw = (t: number) => flushSync(() => root.render(<Scene t={t} />))
const state = { t: 0 }
const tl = gsap.timeline({ paused: true })
tl.to(state, { t: 2, duration: 2, ease: "none", onUpdate: () => draw(state.t) }, 0)
draw(0)
;(window as any).__timelines = (window as any).__timelines || {}
;(window as any).__timelines["main"] = tl
