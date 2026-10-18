// Recharts with fixed sizes and animation off: SVG paths, axes, text measurement, legend.
import { createRoot } from "react-dom/client"
import { flushSync } from "react-dom"
import { Bar, BarChart, CartesianGrid, Legend, Line, LineChart, XAxis, YAxis } from "recharts"

const data = [
  { day: "Mon", views: 1200, likes: 300 }, { day: "Tue", views: 1900, likes: 420 }, { day: "Wed", views: 900, likes: 180 },
  { day: "Thu", views: 2400, likes: 610 }, { day: "Fri", views: 2100, likes: 540 }, { day: "Sat", views: 3100, likes: 880 },
]

function App() {
  return (
    <div style={{ display: "flex", gap: 20, padding: 20, fontFamily: "Helvetica, Arial, sans-serif" }}>
      <LineChart width={440} height={300} data={data}>
        <CartesianGrid strokeDasharray="3 3" />
        <XAxis dataKey="day" />
        <YAxis />
        <Legend />
        <Line type="monotone" dataKey="views" stroke="#4263eb" strokeWidth={2} isAnimationActive={false} />
        <Line type="monotone" dataKey="likes" stroke="#e8590c" strokeWidth={2} isAnimationActive={false} />
      </LineChart>
      <BarChart width={440} height={300} data={data}>
        <CartesianGrid vertical={false} />
        <XAxis dataKey="day" />
        <YAxis />
        <Bar dataKey="likes" fill="#12b886" radius={[4, 4, 0, 0]} isAnimationActive={false} />
      </BarChart>
    </div>
  )
}

flushSync(() => createRoot(document.getElementById("app")!).render(<App />))
