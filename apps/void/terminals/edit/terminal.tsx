// edit: the timeline over what sounds, for cutting a composition on the stage.
import { Controller, Stack, Terminal } from "@raea/terminal";

export default function Edit() {
  return (
    <Terminal name="edit">
      <Stack sizes={[3, 2]}>
        <Controller src="timeline" />
        <Controller src="audio" />
      </Stack>
    </Terminal>
  );
}
