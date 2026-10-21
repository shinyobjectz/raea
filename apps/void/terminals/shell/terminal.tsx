// shell: two shells in the project's folder, side by side.
import { Controller, Split, Terminal } from "@raea/terminal";

export default function Shell() {
  return (
    <Terminal name="shell">
      <Split>
        <Controller src="cli" />
        <Controller src="cli" />
      </Split>
    </Terminal>
  );
}
