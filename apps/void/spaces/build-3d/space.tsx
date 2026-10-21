// Build 3D: a space for building 3D worlds. The object on the stage, the scene's parts on the left, materials across
// the top, the terminal on the right with an agent that knows the 3D SDK.
import { Panel, Space, Stage, Chat, Toolbar } from "@raea/space";

export default function Build3D() {
  return (
    <Space name="Build 3D" agent={{ files: ["AGENTS.md"], skills: ["raea-bevy"] }}>
      <Toolbar src="panels/materials.tsx" side="top" size={52} />
      <Panel src="panels/outliner.tsx" side="left" size={240} />
      <Stage />
      <Chat side="right" />
    </Space>
  );
}
