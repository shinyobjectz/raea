// raea add face-frame: a talking take cropped around the face for the edit's shape, the eyes held at a height you pick.
// The engine reads the face itself (framing="face"); punchIn tightens it (1.15 is a usual punch-in for emphasis).
import { Video } from "@raea/motion";

type Props = {
  src: string;
  startFrom?: number;
  endAt?: number;
  /** 1 = framed to fit; above 1 = tighter */
  punchIn?: number;
  volume?: number;
};

export default function FaceFrame({ src, startFrom, endAt, punchIn = 1, volume = 1 }: Props) {
  return <Video src={src} startFrom={startFrom} endAt={endAt} framing="face" punchIn={punchIn} volume={volume} style={{ width: "100%", height: "100%" }} />;
}
