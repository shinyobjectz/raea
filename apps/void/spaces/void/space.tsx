// void: void's own space. The stage and the player under it, and void's sheets (the objects, chats, the menu, the
// agent's question, voice, files, workspaces), all React panels drawn by raea; the header, the files and the chat
// around it are void's shell (apps/void/shell/, the same in every space: apps/void/shell.feature). Fork it to make void yours:
// /space fork void as mine.
import { Panel, Sheet, Space, Stage } from "@raea/space";

export default function Void() {
  return (
    <Space name="void" agent={{ skills: ["raea"] }}>
      <Stage />
      <Panel src="panels/player.tsx" side="bottom" size={48} />
      {/* void's sheets, over the stage while open: what opened void's own opens these */}
      <Sheet src="sheets/objects.tsx" width={420} height={460} />
      <Sheet src="sheets/chats.tsx" width={380} height={420} />
      <Sheet src="sheets/settings.tsx" width={300} height={340} />
      <Sheet src="sheets/question.tsx" width={560} height={400} />
      <Sheet src="sheets/voice.tsx" width={360} height={340} />
      <Sheet src="sheets/files.tsx" width={460} height={520} />
      <Sheet src="sheets/workspaces.tsx" width={460} height={480} />
    </Space>
  );
}
