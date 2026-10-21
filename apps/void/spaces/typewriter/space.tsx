// Typewriter: the object on the stage and a terminal of the space's own beside it, set like a page in a typewriter.
// What void's terminal does (the conversation, a line to type in) written as an ordinary panel, to show the terminal
// is a part of the interface like any other.
import { Space, Stage, Chat } from "@raea/space";

export default function Typewriter() {
  return (
    <Space name="Typewriter">
      <Stage />
      <Chat src="panels/terminal.tsx" side="right" size={400} />
    </Space>
  );
}
