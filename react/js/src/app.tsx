// The app both paths render: bevy-react's entities (entry.tsx) and react-dom in a page (dom-entry.tsx), compared
// by SSIM (docs/bevy.feature B3: the mirror checked equal before entities become the truth).
import { useState } from "react";

export function Card({ title, n }: { title: string; n: number }) {
  return (
    <div className="card" style={{ padding: 24, margin: 16, borderRadius: 18, background: "#1f2a44", color: "#f4f1ea", fontSize: 40, fontFamily: "Helvetica" }}>
      <span style={{ fontWeight: 700 }}>{title}</span>
      {Array.from({ length: n }, (_, i) => (
        <div key={i} style={{ marginTop: 10, fontSize: 30, opacity: 0.85 }}>item {i}</div>
      ))}
    </div>
  );
}

export function Counter() {
  const [n, setN] = useState(0);
  return (
    <button onClick={() => setN(n + 1)} style={{ margin: 16, padding: "12px 28px", fontSize: 32, borderRadius: 12, border: "none", background: "#ff3366", color: "white" }}>
      <span>{`count ${n}`}</span>
    </button>
  );
}

export function App() {
  return (
    <div id="app" style={{ width: 540, height: 960, background: "#f6efe3", paddingTop: 24 }}>
      <Card title="hello from React in Bevy" n={3} />
      <Counter />
    </div>
  );
}
