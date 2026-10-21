// CSS: a composition's CSS imports and CSS modules reach its page (docs/vite-in-raea.md A2; sdk/react/bench/css/check.sh).
// Left half red with a yellow dot (site.css, its @import and nesting), right half blue (card.module.css: composes,
// color-mix()).
import { Composition, registerRoot } from "@raea/motion";
import "./site.css";
import card from "./card.module.css";

function Css() {
  return (
    <div className="page">
      <div className="left">
        <div className="dot" />
      </div>
      <div className={card.right} />
    </div>
  );
}

registerRoot(() => <Composition id="edit" component={Css} width={400} height={200} fps={30} durationInFrames={30} />);
