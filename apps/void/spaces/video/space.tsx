// Video: today's way of working on a video, as an ordinary space (void's own layout, rebuilt in TSX). A header across
// the top with the object's name and the spaces, the stage, a player under it, the terminal on the right.
import { Panel, Space, Stage, Chat, Toolbar } from "@raea/space";

export default function Video() {
  return (
    <Space name="Video" agent={{ files: ["AGENTS.md"], skills: ["raea"] }}>
      <Toolbar src="panels/header.tsx" side="top" size={44} />
      <Stage />
      <Panel src="panels/player.tsx" side="bottom" size={48} />
      <Chat side="right" />
    </Space>
  );
}
