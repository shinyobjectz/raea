// The app on the appscreen bench's screen (edit.tsx): a button that counts presses and rings while the pointer is over
// it, and a bar its own timer grows 8 px every 100 ms.
import { useEffect, useState } from "react";

export default function Kit({ width, height }: { width: number; height: number }) {
  const [n, setN] = useState(0);
  const [hot, setHot] = useState(false);
  const [tick, setTick] = useState(0);
  useEffect(() => {
    const t = setInterval(() => setTick((v) => v + 1), 100);
    return () => clearInterval(t);
  }, []);
  return (
    <div style={{ position: "fixed", left: 0, top: 0, width, height, background: "#10151f", fontFamily: "Inter, sans-serif" }}>
      <button
        onClick={() => setN((v) => v + 1)}
        onPointerEnter={() => setHot(true)}
        onPointerLeave={() => setHot(false)}
        style={{ position: "absolute", left: 80, top: 50, width: 160, height: 80, boxSizing: "border-box", borderRadius: 12, border: `6px solid ${hot ? "#ffd23f" : "#10151f"}`, background: n > 0 ? "#22b573" : "#3a6cff", color: "#ffffff", fontSize: 24 }}
      >
        {n > 0 ? `Pressed ${n}` : "Press"}
      </button>
      <div style={{ position: "absolute", left: 0, bottom: 0, height: 12, width: Math.min(width, tick * 8), background: "#ff5d8f" }} />
    </div>
  );
}
