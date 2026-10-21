// perform: play the edit with the dino over the timeline (each cut an obstacle; a crash pauses on it).
import { Controller, Stack, Terminal } from "@raea/terminal";

export default function Perform() {
  return (
    <Terminal name="perform">
      <Stack sizes={[3, 2]}>
        <Controller src="dino" />
        <Controller src="timeline" />
      </Stack>
    </Terminal>
  );
}
