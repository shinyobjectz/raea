// @raea/motion/test: the test clock. It uses react-dom's server renderer, which the engine's V8 can't run, so it
// lives here and never in the main entry (spec: "The test clock").
import type { ReactNode } from "react";
// @ts-expect-error react-dom ships no type declarations; the one function used is typed below
import * as server from "react-dom/server";
import { Clock, type InputTimeline, type Perception, type TimelineWord } from "./clock";

const renderToStaticMarkup: (node: ReactNode) => string = (server as { renderToStaticMarkup(node: ReactNode): string })
  .renderToStaticMarkup;

export * from "./index";
export { clearRegistry as resetRegistry } from "./registry";

export interface RenderFrameOptions {
  frame: number;
  fps: number;
  width: number;
  height: number;
  durationInFrames: number;
  id?: string;
  defaultProps?: Record<string, unknown>;
  words?: readonly TimelineWord[];
  perception?: Perception;
  input?: InputTimeline;
}

/** Renders a tree at one frame without a window and returns its markup. */
export function renderFrame(node: ReactNode, opts: RenderFrameOptions): string {
  return renderToStaticMarkup(<Clock {...opts}>{node}</Clock>);
}
