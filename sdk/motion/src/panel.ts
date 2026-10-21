// Panels: a void space's panels and toolbars are components played live like games (void.feature: A space is the
// interface). A panel's actions are what both a person and the agent can do with it: a click calls one in the panel's
// step, and the agent's panel_action tool hands one in as input, so both run the same code.
//
//   export const actions = {
//     pick: { about: "Pick a row of the outliner", args: { row: "number, from 0" }, run: (s, a) => ({ ...s, picked: a.row }) },
//   };
//   const step = (s, input) => applyActions(actions, s, input);
import type { FrameInput } from "./clock";

/** One action: what it does (for the agent), its arguments by name with a few words each, and what it does to the
 *  panel's state. */
export interface PanelAction<S, A = any> {
  about: string;
  args?: Record<string, string>;
  run: (state: S, args: A) => S;
}
export type PanelActions<S> = Record<string, PanelAction<S>>;

/** The prefix void puts on an action handed in as input: a key `action:NAME ARGS_JSON`, held for one frame. */
export const ACTION_KEY = "action:";

/** The actions handed in this frame, in order. */
export function actionsIn(input: FrameInput): { name: string; args: any }[] {
  const out: { name: string; args: any }[] = [];
  for (const k of input.keys ?? []) {
    if (!k.startsWith(ACTION_KEY)) continue;
    const rest = k.slice(ACTION_KEY.length);
    const sp = rest.indexOf(" ");
    const name = sp < 0 ? rest : rest.slice(0, sp);
    let args: any = {};
    if (sp >= 0) {
      try {
        args = JSON.parse(rest.slice(sp + 1));
      } catch {
        args = {};
      }
    }
    out.push({ name, args });
  }
  return out;
}

/** `state` with this frame's handed-in actions run on it, in order (an unknown name is skipped). */
export function applyActions<S>(actions: PanelActions<S>, state: S, input: FrameInput): S {
  let s = state;
  for (const a of actionsIn(input)) {
    const act = actions[a.name];
    if (act) s = act.run(s, a.args);
  }
  return s;
}

/** The prefix of the key void holds on the stage for each panel that publishes (`data-void-publish` on an element):
 *  `space:PANEL JSON`, held while the value stands, so the object on the stage reads what the panels picked. */
export const SPACE_KEY = "space:";

/** What a panel of the open space publishes, as the object on the stage sees it this frame (undefined: nothing). */
export function spaceIn<T = any>(input: FrameInput, panel: string): T | undefined {
  const prefix = SPACE_KEY + panel + " ";
  for (const k of input.keys ?? []) {
    if (!k.startsWith(prefix)) continue;
    try {
      return JSON.parse(k.slice(prefix.length)) as T;
    } catch {
      return undefined;
    }
  }
  return undefined;
}

/** The prefix of the keys void hands every panel for what it knows: `void:transcript` (the conversation's last lines,
 *  [{ who: "you" | "void" | "tool" | "note", text, live }]), `void:object` (the object on the stage: { name, kind }) and
 *  `void:stage` (what the object publishes with data-void-publish, as a panel's publishes reach the object) and
 *  `void:player` (the stage's player: { frame, frames, rate, playing }; frames is 0 for a game, which runs on).
 *  Handed in only on the frame it changes (and when the panel opens), so keep it in state:
 *  `useGameState([], (s, input) => voidIn(input, "transcript") ?? s)`. A panel standing in for the terminal sends by
 *  publishing { send: { n, text } } (a higher n is a new message); a panel drives the player by publishing
 *  { control: { n, do: "play" | "pause" | "seek", frame } } the same way. */
export const VOID_KEY = "void:";

/** What void tells the panels, by name ("transcript", "object"); undefined until it has. */
export function voidIn<T = any>(input: FrameInput, what: string): T | undefined {
  const prefix = VOID_KEY + what + " ";
  for (const k of input.keys ?? []) {
    if (!k.startsWith(prefix)) continue;
    try {
      return JSON.parse(k.slice(prefix.length)) as T;
    } catch {
      return undefined;
    }
  }
  return undefined;
}
