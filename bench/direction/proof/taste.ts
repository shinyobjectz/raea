// The proof project's taste (docs/direction.feature DR4): its rules, each a line with the check that holds it. A .ts
// check by its path from here, with its values, or one of raea's forms (text-above), which the two taste cases pass
// with --taste. `raea check FILE.tsx --only ID` runs one.
import { taste } from "raea/brief";

export default taste({
  title: "Proof taste",
  sections: [
    {
      heading: "What the pieces are held to",
      rules: [
        { text: "Sung lines are flat 2D type, inside the frame, behind the subject", check: "checks/lyric.ts" },
        { text: "Cuts land on a word's end", check: "checks/cut.ts" },
        { text: "Cues are heard where they are placed", check: "checks/cue.ts" },
        { text: "Captions clear the button band", check: "checks/caption.ts", values: { band: 0.8 } },
        { text: "The rider turns with its seat and is in no part of it", check: "checks/rider.ts", values: { rider: "rider", seat: "seat" } },
      ],
    },
    { heading: "Text", rules: [{ text: "Captions sit above three quarters of the frame.", check: { form: "text-above", value: 75 } }] },
  ],
});
