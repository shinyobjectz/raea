// Vector: a space for drawing in vectors. Tools and colours across the top, the drawing's layers on the left, the
// drawing on the stage (draw on it with the pointer), the terminal on the right with an agent that can draw too.
import { Panel, Space, Stage, Chat, Toolbar } from "@raea/space";

export default function Vector() {
  return (
    <Space name="Vector" agent={{ files: ["AGENTS.md"] }}>
      <Toolbar src="panels/tools.tsx" side="top" size={52} />
      <Panel src="panels/layers.tsx" side="left" size={220} />
      <Stage />
      <Chat side="right" />
    </Space>
  );
}
