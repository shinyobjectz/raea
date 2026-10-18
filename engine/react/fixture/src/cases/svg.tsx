// SVG from React: a bar chart and a donut, style objects, camelCase SVG props, text.
import { createRoot } from "react-dom/client"
import { flushSync } from "react-dom"

const data = [["Mon", 12], ["Tue", 19], ["Wed", 7], ["Thu", 23], ["Fri", 16]] as const
const max = 25

function Bars() {
  return (
    <svg width="420" height="260" viewBox="0 0 420 260">
      <line x1="40" y1="220" x2="400" y2="220" stroke="#999" strokeWidth={1} />
      {data.map(([d, v], i) => {
        const h = (v / max) * 190
        return (
          <g key={d} transform={`translate(${60 + i * 70},0)`}>
            <rect x={0} y={220 - h} width={40} height={h} rx={4} fill="#4263eb" fillOpacity={0.4 + v / 50} />
            <text x={20} y={240} textAnchor="middle" fontSize={13} fill="#333">{d}</text>
            <text x={20} y={214 - h} textAnchor="middle" fontSize={12} fontWeight={700} fill="#222">{v}</text>
          </g>
        )
      })}
    </svg>
  )
}

function Donut({ pct }: { pct: number }) {
  const r = 70, c = 2 * Math.PI * r
  return (
    <svg width="200" height="200" viewBox="0 0 200 200" style={{ marginLeft: 20 }}>
      <circle cx={100} cy={100} r={r} fill="none" stroke="#e9ecef" strokeWidth={22} />
      <circle cx={100} cy={100} r={r} fill="none" stroke="#12b886" strokeWidth={22} strokeLinecap="round"
        strokeDasharray={`${c * pct} ${c}`} transform="rotate(-90 100 100)" />
      <text x={100} y={108} textAnchor="middle" fontSize={26} fontWeight={700} fill="#222">{Math.round(pct * 100)}%</text>
    </svg>
  )
}

const root = createRoot(document.getElementById("app")!)
flushSync(() => root.render(<div style={{ display: "flex", alignItems: "center", padding: 24 }}><Bars /><Donut pct={0.68} /></div>))
