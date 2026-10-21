# browser

`raea-browser`: the agent's web browser, exposed to TypeScript as the `browser.run` native call. It takes the same
command words as the `agent-browser` CLI (open, snapshot, click, fill, get text, screenshot, eval, ...) and answers
with the text that CLI would print.

There are two backends behind the same words:

- Chrome over CDP, through agent-browser (Apache-2.0, vendored in `vendor/agent-browser` and run in process). This is
  the default `chrome` feature.
- raea's own engine (Blitz, Stylo and the page scripts on JavaScriptCore), with no Chrome, a virtual clock and no
  network unless allowed. It works where Chrome can't run, such as the iPhone.

`ts/` is the TypeScript wrapper; `conformance/` holds its tests and test pages. Depends on
[`engine/html/`](../html) and [`engine/native/`](../native).

Status: working; the Blitz backend covers less of the web than Chrome.
