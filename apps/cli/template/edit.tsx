// The edit: a React composition rendered by raea (`raea render`). The clips come from cuts.ts (`raea cut` writes it),
// the parts from components/ (`raea add` copies them in; they're yours to change), the look from brand/brand.css.
// Change anything here: layout, timing, what plays when. `raea check` renders it and says what's wrong.
import { AbsoluteFill, Composition, Series, Video, registerRoot } from "@raea/motion";
import { cuts, edit, frames, total } from "./cuts";
import { css } from "./styles.gen";

function Edit() {
  return (
    <AbsoluteFill className="bg-brand-bg">
      <style>{css}</style>
      <Series>
        {cuts.map((c, i) => (
          <Series.Sequence key={i} durationInFrames={frames(c)}>
            {/* a talking take: <FaceFrame>; a screen recording: <ScreenFit> (`raea frame TAKE` says which) */}
            <Video src={c.src} startFrom={Math.round(c.from * edit.fps)} endAt={Math.round(c.to * edit.fps)} style={{ width: "100%", height: "100%", objectFit: "cover" }} />
          </Series.Sequence>
        ))}
      </Series>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="edit" component={Edit} width={edit.width} height={edit.height} fps={edit.fps} durationInFrames={Math.max(1, total)} />);
