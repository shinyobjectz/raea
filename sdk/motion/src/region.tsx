// Placement from perception, and the proof (spec: "Placement from perception", "The proof").
import { createContext, useContext, type CSSProperties, type ReactNode } from "react";
import { useClockState, type Perception, type Rect } from "./clock";

export type RegionName = "auto" | "top" | "bottom" | "center" | "full";
const NAMES: readonly RegionName[] = ["auto", "top", "bottom", "center", "full"];

/** The band a Region occupies, in composition px. */
export interface RegionBox {
  name: RegionName;
  x: number;
  y: number;
  w: number;
  h: number;
}

/** The smallest band height a Region accepts, as a share of the composition's height. */
const MIN_BAND = 0.08;

interface Band {
  name: Exclude<RegionName, "auto">;
  top: number;
  bottom: number;
}

/** Works out the vertical band each name stands for, given perception and the composition size. */
export function regionBand(name: RegionName, width: number, height: number, perception: Perception, margin = 0.06): Band {
  const m = Math.round(margin * width);
  const edgeTop = perception.safe?.top ?? MIN_BAND * height;
  const edgeBottom = perception.safe?.bottom ?? (height > width ? 0.8 : 0.92) * height;
  const face = perception.face;
  const top: Band = { name: "top", top: edgeTop, bottom: face ? face.y - m : 0.45 * height };
  const bottom: Band = { name: "bottom", top: face ? face.y + face.h + m : 0.55 * height, bottom: edgeBottom };
  switch (name) {
    case "top":
      return top;
    case "bottom":
      return bottom;
    case "center":
      return { name: "center", top: 0.33 * height, bottom: 0.67 * height };
    case "full":
      return { name: "full", top: edgeTop, bottom: edgeBottom };
    case "auto":
      return bottom.bottom - bottom.top > top.bottom - top.top ? bottom : top;
  }
}

const RegionContext = createContext<RegionBox | null>(null);

/** The Region this is rendered inside: its name and box in composition px. */
export function useRegion(): RegionBox {
  const box = useContext(RegionContext);
  if (!box) throw new Error("useRegion was called outside a <Region>");
  return box;
}

export interface RegionProps {
  name?: RegionName;
  margin?: number;
  style?: CSSProperties;
  className?: string;
  children?: ReactNode;
}

const px = (n: number) => `${+n.toFixed(3)}px`;

/** A box at the band its name stands for, clear of the face perception found at this frame. */
export function Region({ name = "auto", margin = 0.06, style, className, children }: RegionProps) {
  if (!NAMES.includes(name)) throw new RangeError(`Region: name must be one of ${NAMES.join(", ")}, got "${name}"`);
  if (typeof margin !== "number" || !(margin >= 0 && margin < 0.5)) {
    throw new RangeError(`Region "${name}": margin must be a share of the width in [0, 0.5), got ${margin}`);
  }
  const clock = useClockState("Region");
  const { width, height } = clock.config;
  const face = clock.perception.face;
  const band = regionBand(name, width, height, clock.perception, margin);
  const m = Math.round(margin * width);
  const box: RegionBox = { name, x: m, y: band.top, w: width - 2 * m, h: band.bottom - band.top };

  const attrs = {
    className,
    "data-region": name,
    "data-band": band.name,
    "data-band-top": +band.top.toFixed(3),
    "data-band-bottom": +band.bottom.toFixed(3),
  };

  if (box.h < MIN_BAND * height) {
    const left = Math.max(0, Math.round(box.h));
    const why = face ? `the face leaves ${left} px` : `the band is ${left} px`;
    return <div {...attrs} data-refused={`no room ${band.name}: ${why}`} style={{ display: "none" }} />;
  }

  const vars: Record<string, string> = { "--region-width": px(box.w), "--region-height": px(box.h) };
  if (face) {
    vars["--face-top"] = px(face.y);
    vars["--face-bottom"] = px(face.y + face.h);
    vars["--face-left"] = px(face.x);
    vars["--face-right"] = px(face.x + face.w);
  }
  const boxStyle = {
    ...style,
    ...vars,
    position: "absolute",
    left: box.x,
    top: box.y,
    width: box.w,
    height: box.h,
  } as CSSProperties;
  return (
    <div {...attrs} style={boxStyle}>
      <RegionContext.Provider value={box}>{children}</RegionContext.Provider>
    </div>
  );
}

export interface ProofPart extends Rect {
  name: string;
  /** the band the part must fit in, [top, bottom] in px */
  band?: [number, number];
}

export interface ProofInput {
  width: number;
  height: number;
  face?: Rect;
  parts: ProofPart[];
}

/** The share of the face's area a part may cover before it's a fault. */
const FACE_COVER_LIMIT = 0.08;

const num = (n: number) => `${+n.toFixed(2)}`;

/** Checks measured part boxes; returns one fault per problem, empty when all is well. */
export function proofBoxes({ width, height, face, parts }: ProofInput): string[] {
  const faults: string[] = [];
  for (const part of parts) {
    const { name, x, y, w, h } = part;
    if (x < 0 || y < 0 || x + w > width || y + h > height) faults.push(`${name} runs off the frame`);
    if (part.band) {
      const room = part.band[1] - part.band[0];
      if (h > room) faults.push(`${name} is ${num(h)} px tall and its room is ${num(room)} px`);
    }
    if (face && face.w > 0 && face.h > 0) {
      const ox = Math.max(0, Math.min(x + w, face.x + face.w) - Math.max(x, face.x));
      const oy = Math.max(0, Math.min(y + h, face.y + face.h) - Math.max(y, face.y));
      const share = (ox * oy) / (face.w * face.h);
      if (share > FACE_COVER_LIMIT) {
        faults.push(
          `${name} covers ${Math.round(share * 100)}% of the face at x ${num(face.x)} y ${num(face.y)} w ${num(face.w)} h ${num(face.h)}`,
        );
      }
    }
  }
  return faults;
}
