Feature: React compositions
  raea renders pages that run real react-dom, with no browser: React runs in Boa (blitz-vibey-script)
  against the blitz-dom document, then Blitz styles, lays out and paints it. The gate is tests/parity/react
  against the composition format's Chrome 152, like every other parity group. Sources and builders live in react/.

  Background:
    Given the React fixtures are built with `cd react/fixture && bun install && python3 build-cases.py`
    And react/eco has `bun install` and `./node_modules/.bin/tailwindcss -i src/tw.css -o dist/tw.css` run
    And the Chrome references come from `zsh bench/parity-refs.sh react/`

  Scenario: What the parity group covers (bench/parity.py react/, 2026-10-13)
    Then these cases pass at SSIM >= 0.995:
      | case     | what it exercises                                                           | SSIM   |
      | board    | React 18 createRoot + flushSync, keyed lists, flexbox                       | 0.9975 |
      | hooks    | useLayoutEffect measuring a ref, useEffect state, useReducer, context, memo | 1.0000 |
      | timeline | React re-rendering every frame from a GSAP timeline's onUpdate              | 0.9999 |
      | svg      | SVG from JSX: camelCase props, transforms, text, style objects              | 0.9996 |
      | emotion  | React 19 + Emotion in production (CSSOM insertRule, keyframes)              | 0.9995 |
      | recharts | React 19 + Recharts 3 (line, bar, axes, legend)                             | 0.9959 |
    And shadcn (React 19, Tailwind v4, Radix Tabs/Progress/Avatar, a Dialog open in a portal) is at 0.9868

  Scenario: The web platform layer
    Given vendor/blitz-vibey-script/src/web.js is evaluated after geometry.js in every document
    Then react/probe/index.html passes 154 of 157 browser API checks (111 before)
    And the layer supplies: Mouse/Pointer/Keyboard/Focus/Input/Wheel event constructors, instanceof for
      HTMLInputElement/SVGElement and friends (answered from tag and namespace, since all elements share one
      prototype), insertAdjacentElement/Text, getRootNode, compareDocumentPosition, toggleAttribute, *AttributeNS,
      attributes, tabIndex, inert, checkVisibility, offsetParent, click(), pointer capture no-ops, HTMLCollection.item,
      getScreenCTM/getCTM/createSVGPoint, document.createRange, constructable stylesheets + adoptedStyleSheets,
      cookie, visibilityState, hasFocus, localStorage/sessionStorage (in memory), crypto (seeded, so renders are
      repeatable), URL.searchParams, navigator.language(s), visualViewport, reportError, DOMParser,
      Intl.RelativeTimeFormat (English)
    And Boa's Intl.DateTimeFormat bug (year "numeric" printed as two digits with month/day) is corrected
    And Element.getAttributeNames is native (src/dom/element.rs)
    And SVG <text> getBBox measures the text in an off-screen span (src/gsap_shim.js textBBox), because Blitz
    paints an inline SVG as one image and its <text> has no layout box

  Scenario: Focus rings, as Radix dialogs show them
    Given a script calls focus() on an element and no pointer input happened
    Then :focus-visible matches it (vendor/blitz-dom/src/stylo.rs), as in Chrome
    And the UA sheet gives it `outline: auto 1px rgb(0, 95, 204)` (Chrome's html.css)
    And blitz-paint draws outline-style auto as Blink does (outline_painter.cc PaintFocusRing): two 2px stroked
      rounded rects, white 1px further out then the outline colour, offset = outline-offset - 1px when every border
      edge is >= 1px, corner radius = the author's border radius but at least 3px

  Scenario: What is left
    Then these are open:
      | gap                                    | why                                                                       |
      | shadcn heading/tab text a little wide  | the composition format's Inter faces for Tailwind's font-sans stack (font parity)     |
      | attachShadow                           | no shadow DOM in blitz-dom                                                |
      | Element.animate / Web Animations       | absent on purpose: Motion and others fall back to JS animation            |
      | performance.now() is frozen (0)        | the deterministic clock; libraries timing themselves see no time pass     |
      | Boa is slow for big React trees        | use the `v8` feature (docs/v8.feature): 32x faster on a re-render bench   |

  Scenario: Speed (M4, warm, one process per snapshot)
    Then a 300-card board snapshots in 0.30 s (0.13 s as static HTML) and a 1000-card board in 1.05 s (0.22 s)
    And the timeline case renders 60 frames, React re-rendering each one, at 43 fps on one worker
    And React's renderToString in Boa is slower than its client render, so the DOM bindings are not the bottleneck
