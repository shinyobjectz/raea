// raea add screen-fit: a landscape screen recording shown whole in a vertical frame, over a soft fill of itself, instead
// of a cover crop that cuts its sides off (every app render on 2026-10-16 lost Jetflow's UI that way). `raea frame`
// says when a take wants this. Leaves the space above and below for a headline and captions.
import { AbsoluteFill, Video, useVideoConfig } from "@raea/motion";

type Props = {
  src: string;
  /** source frames, as <Video> takes them */
  startFrom?: number;
  endAt?: number;
  /** the recording's own aspect (width / height); 16/9 when not given */
  aspect?: number;
  /** where its centre sits, as a share of the frame's height (0.5 = the middle) */
  y?: number;
};

export default function ScreenFit({ src, startFrom, endAt, aspect = 16 / 9, y = 0.5 }: Props) {
  const { width, height } = useVideoConfig();
  const h = width / aspect;
  const top = Math.max(0, Math.min(height - h, height * y - h / 2));
  return (
    <AbsoluteFill className="bg-brand-bg">
      <Video src={src} startFrom={startFrom} endAt={endAt} muted style={{ position: "absolute", inset: 0, width: "100%", height: "100%", objectFit: "cover", opacity: 0.35, filter: "blur(40px)" }} />
      <Video src={src} startFrom={startFrom} endAt={endAt} style={{ position: "absolute", left: 0, top, width, height: h, objectFit: "contain" }} className="rounded-brand shadow-2xl" />
    </AbsoluteFill>
  );
}
