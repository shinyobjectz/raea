# conformance

The UI conformance suite: the runner for the contract in `sdk/ui/conformance/conformance.feature`. It launches a UI
substrate headless in a throwaway world (its own HOME, MCP port and copy of `fixture/`, the chat on the echo stub),
drives it only through MCP `input`, reads it only through MCP `ui`, and writes `results.jsonl` and `summary.json`.

It's its own Cargo workspace (it doesn't link the engine). Build and test it through the lane:

    cd sdk/ui/conformance
    lane build -- cargo build --release -j 6
    lane build -- cargo test --release -j 6        # the runner's own tests, on the fake substrate (fake/)

Run it:

    target/release/conformance --tier quick                                  # the feed on target/release/raea-page-host
    target/release/conformance --no-build --only 'scroll/*'                  # apps/app's last build as it is
    target/release/conformance --tier full --only 'pointer/*' --keep         # every size; keep worlds and logs
    target/release/conformance --app ~/raea-trial/a/apps/app                 # another checkout of the app (a release's)

The cases are written against the app's demo world (the Ides of March, Pong, the reel and the tutorial), which a
build has only when asked for since 11-14 (a shipped app has none): the runner builds it with `RAEA_FIXTURES=1` into
`blitz/out-conformance`, and `--no-build` reads that build (else an older app's `blitz/out`). raea-page-host is only
the window: the page's `/host/*` (onboarding's state, workspaces, kept items) is the feed's host's, so each world starts
the app's `host/node.ts` on a free port first and points the page at it (`RAEA_PAGE_HOST`), as raea.app does; without
it the page waits on "raea is starting" and every case fails at its first step.

Exit 0: every case passed or was only slow; 1: a case failed; 2: the run couldn't start.

## Cases

`cases/AREA/NAME.toml`: a window size (or `windows` for the full tier), steps (`input`, `wait`, `expect`, `snap`,
`write`, `repeat`, any with `if`), and a budget in frames. The step kinds are documented at the top of `src/case.rs`,
the predicates in `src/pred.rs`.

## When the runner reads the tree

A substrate can report the window at its new size frames before its panels have laid out at it (the current stack:
`middle` at 752x522 or 765x614 against 800x860 once settled). So the runner waits for the tree to *settle* before:

- the `start` snapshot (after the window has the case's size and anything is in it),
- each `snap` step,
- resolving an `input` step's targets (`at`, `from`, `to`), unless the step before was also an input: consecutive
  inputs are one gesture (two quick presses) and aren't separated.

Settled means no panel under `panels` has `stale: true`, and every node's id, rect and visibility, the window, and the
panels' boxes and textures are the same on two consecutive frames (or, from a substrate presenting no new frame, on one
frame for 250 ms). A panel that stays flagged stale while nothing at all changes (its flag, every rect, the window) for
1.5 s counts as settled too, since waiting can't help; the result notes "settled with panels stale ... : NAME (why)",
and the stale panel's nodes report `visible: false`, so a case that needs them fails on its own predicates. The wait
is bounded by `--settle-ms` (default 10000; 0 turns it off): a tree that doesn't settle
fails the case and names what kept moving. The settle happens before an input is sent, so it never counts toward a
case's latency, which is frames from the input to the first tree where the expectation holds.

## CI

`bench/ci.sh` builds raea-page-host and this runner and runs `conform:quick` on every push to main and `conform:full` nightly
(`.github/workflows/ci.yml`, the m5pro runner). Results land in `~/Library/Logs/raea/ci/runs/<run>/conform/`.

## Disputed: awaiting decision

These cases' expectations are Shane's call and are not changed by the runner until he decides:

- `review/j-after-approve`: expects `feed.card.3` in the tree at 1440x900 before the first review (whether cards
  below the first screen, card.2 and card.3, must be on screen / in the tree), and J after approving card.1 to open
  the item card.3 shows.
- `scroll/left-list-pixel`, `scroll/left-list-line`: expect the left panel's feeds list (`left.feeds`) to overflow
  and scroll at 900x700.
