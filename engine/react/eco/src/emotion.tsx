/** @jsxImportSource @emotion/react */
// CSS-in-JS: Emotion in production injects rules with CSSOM insertRule ("speedy"), not text nodes.
import { createRoot } from "react-dom/client"
import { flushSync } from "react-dom"
import styled from "@emotion/styled"
import { css, keyframes } from "@emotion/react"

const Card = styled.div<{ tone: string }>`
  margin: 18px 28px; padding: 16px 20px; border-radius: 12px;
  background: ${(p) => p.tone}; font: 18px Helvetica, Arial, sans-serif;
  display: flex; align-items: center; justify-content: space-between;
  &:hover { outline: 2px solid red; }
  @media (min-width: 600px) { box-shadow: 0 4px 14px rgba(0,0,0,.12); }
`
const Badge = styled.span`
  display: inline-block; padding: 4px 10px; border-radius: 999px; background: #212529; color: white; font-size: 13px;
`
const pulse = keyframes`from { opacity: .4 } to { opacity: 1 }`

function App() {
  return (
    <div>
      {[["#fff4e6", "Draft brief", "2 days"], ["#e7f5ff", "Review copy", "today"], ["#ebfbee", "Ship", "done"]].map(([tone, t, b]) => (
        <Card key={t} tone={tone}>
          <span css={css`font-weight: 700; letter-spacing: -0.01em;`}>{t}</span>
          <Badge>{b}</Badge>
        </Card>
      ))}
      <div css={css`margin: 18px 28px; height: 12px; border-radius: 6px; background: #7048e8; animation: ${pulse} 1s linear both;`} />
    </div>
  )
}

flushSync(() => createRoot(document.getElementById("app")!).render(<App />))
