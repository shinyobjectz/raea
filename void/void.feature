Feature: raea void: a black box for making anything in raea, by talking
  Status 2026-10-15: the shell, second build. A testing ground for an agent harness built for raea, about raea, and
  the part of raea that goes into the host app for UGC editing, so its pieces (brain, tools, evals, voice) stay composable.
  The founder, 2026-10-15: "a voice chat based black box … the desktop app substrate for generating basically anything in
  raea". On the look, later: a terminal, but one people who don't use terminals understand: no JS libraries, Rust
  where it can be, and no `ls` or bash jargon on screen.

  Code: raea/void (crate raea-void, binary `void`, Dioxus 0.7 desktop). Library = the voice (src/voice); binary = the
  shell (app, ui, chooser, term, narrate, route, intent, brain, workspace, spaces, stage, frames, mcp, telemetry, eval).

  Rule: The window is a black canvas, a stage and a terminal
    Scenario: What's on screen
      Then the window has no title bar: the traffic lights sit over the black canvas
      And the open space's program fills the stage
      And the terminal docks at the bottom, left or right (/layout, or the settings menu's dock icons), kept in
        ~/.raea-void/settings.json
      And its header holds the cat (settings; a right-click opens them too), the turning cube (opens Spaces), the
        four-bar mic, the space's name, stop, and the pane's two tabs: Terminal and Files

    Scenario: The terminal
      Then it's a real terminal emulator (alacritty_terminal) drawing void's own lines, not a shell
      And what I say or type shows as mine; void's replies type in; what it's doing shows as process lines with a verb
        ("write", "check", "look", "measure"), a few detail lines (what changed, which checks fail), and a ✓ or ⚠
      And speech heard so far shows dim while I talk, and void's spoken words show in violet
      And text is selectable (⌘C), the transcript is kept per space (terminal.json), and redraws are at most 30 a second

  Rule: void works in a folder I pick, the way Claude Code does: the folder is the workspace
    The founder, 2026-10-17: "I pick a folder to start in, and that's the workspace I picked … spaces are like folders of
    a sort of monorepo within it (not a full monorepo) … an option for 'all spaces', meaning the root level."

    Scenario: A workspace's layout (workspace.rs)
      Then a workspace is any folder, laid out as a light monorepo:
        | path                    | what                                                                           |
        | .void/workspace.json    | {"void": 1, "name", "created", "settings"}; marks the folder as a workspace     |
        | .void/terminal.json     | the All spaces transcript                                                       |
        | AGENTS.md               | the layout, for an agent at the root (new workspaces only, never overwritten)  |
        | spaces/<slug>/          | one space each, as `raea init` lays it out (below)                              |
        | brand/                  | optional: brand kits any space can copy (brand/<name>/brand.css)               |
        | shared/components/      | optional: components any space can copy                                         |
        | media/                  | optional: a shared footage library; spaces link the takes they use             |
      And spaces stay separate: a space reads only its own folder, so it renders the same wherever it's moved
      And spaces are listed most recently opened first (each space.json's opened), so the root keeps no order

    Scenario: Launching
      When I start void
      Then the folder given on the command line opens (`void .`, `void ~/ads`)
      Or else the last workspace reopens if it still exists and was opened in the last two weeks
      Or else the chooser covers the window: the recent workspaces (~/.raea-void/recent.json: path, name, opened;
        newest first, twelve kept, missing folders not shown), then "Open folder…" (⌘O, macOS's folder picker) and
        "New workspace…" (⌘N, a save dialog: name it and pick where; the folder is made)
      And ↑ ↓ and return pick a recent one, × takes one off the list (the folder stays)
      And the cat's menu has "Switch workspace…", which brings the chooser back; escape returns to the open workspace

    Scenario: Opening a folder
      When I open a folder void hasn't seen
      Then only .void/ and spaces/ are added to it; nothing else in it is touched
      And a new workspace in an empty folder also gets AGENTS.md, brand/, shared/components/ and media/
      And its most recent space opens, or an Untitled one is made to name itself from the first request
      And the agent starts over in it: its spaces and its shell are the workspace's

    Scenario: The spaces from before workspaces
      Given spaces in the old global folder (~/.raea-void/spaces)
      Then the chooser offers it as "Legacy spaces", a workspace as it is (its spaces sit directly in the root)
      And nothing in it is moved, renamed or deleted

    Scenario: All spaces
      When I open "All spaces" (first in the spaces sheet, or "/open all")
      Then the agent and its shell work at the workspace's root, across every space
      And its transcript is kept in .void/terminal.json

  Rule: Spaces are where work lives; opening one swaps its program onto the stage
    Scenario: A space
      Then a space is a folder in the workspace's spaces/ with a space.json and a React + TypeScript project as
        `raea init` makes it (edit.tsx, cuts.ts, styles.gen.ts, AGENTS.md, components/, media/; brand/ once `raea
        brand` runs), written from raea/cli's own template and checked against it byte for byte when bun is there
      And while the default agent still edits rrl, an empty edit.rrl beside it; older spaces' edit.rrl (raea/rrl's
        language, every move recorded in edit.rrl.moves) and brief.feature still load
      And with no workspace open (evals, `void tool`) spaces are the legacy ~/.raea-void/spaces (RAEA_VOID_HOME moves
        the home)
      And a space made from a first request names itself from it ("make a fun bouncing ball" -> "Fun bouncing ball")

    Scenario: The spaces sheet
      When I click the turning cube (or /spaces)
      Then a sheet lists every space, most recently opened first; typing filters it, and return makes a new space
        with that name when none matches; the + button makes one too

    Scenario: The space's files
      When I pick the Files tab in the pane's header (Terminal and Files are the space's two tabs, wherever the pane
        docks; icons when the pane is narrow, icons and names when it's wide; the choice is kept in settings.json)
      Then the pane lists the open space's files by kind (video, images, audio, 3D, documents, other), each with a
        thumbnail (Quick Look: a picture, a video's poster frame, a PDF's first page; the extension otherwise), its name,
        length or size, and whether it's linked or transcribed
      And + opens the native picker for several files, and files dropped anywhere on the window come in too
      And video, audio and images land in media/, 3D (glb gltf obj fbx usdz usd stl ply) in models/, documents (pdf md
        txt rtf doc docx pages key csv) in docs/, anything else in assets/ (src/assets.rs)
      And a file under 256 MB is copied, a larger one linked; nothing is written through a link, and removing a linked
        file removes the link, never what it points to
      And video and audio are transcribed as import_media does (<stem>.words.json beside them), each import is a line
        in the terminal, and the agent's next turn in that space is told what was added or removed
      And nothing is uploaded: footage stays on this Mac
      And per file: show in Finder, and remove (× then Remove)

    Scenario: Files are found by what they are
      When I type in the Files tab's search ("woman holding a coffee cup", "talks about the price")
      Then names and words match at once, and a moment later files match by meaning: pictures (an image, three stills
        of a video, a PDF's or model's Quick Look picture) by SigLIP 2, speech and text documents in short windows by
        nomic-embed-text v1.5, each hit saying why ("seen: still at 8.5 s", "said at 4:23: …")
      And the agent searches the same way with its search_assets tool
      And the index is the space's .assets-index.json, remade per file when it or its transcript changes

    Scenario: The first launch
      Given no recent workspaces
      Then the chooser shows (with "Legacy spaces" when the old global folder has any); nothing is seeded

  Rule: The stage is raea itself, kept warm
    Scenario: Drawing a frame
      Then one resident `rrl serve` draws the open program as raw RGBA at preview quality (no PNG encode, no MSAA or
        shadow maps), halved, kept in a frame cache (superseded: the engine's native stage draws the object in the
        chrome's own frame; the webview and its void:// protocol were deleted 2026-10-18)
      And playback plays a preview: the program rendered to H.264 with its sound by `rrl render`, one file per edit
        version under ~/.raea-void/previews, the newest 8 kept

  Rule: Anything typed or said is a command; a few run at once, the rest go to the brain
    Scenario Outline: Commands that never wait for the model
      When I type "<command>"
      Then <what happens>
      Examples:
        | command            | what happens                                       |
        | /spaces            | the spaces sheet opens                             |
        | /new Coffee ad     | a space "Coffee ad" is made and opened             |
        | /open hook         | the best-matching space is opened                  |
        | /voice, /voice alba| the voice sheet opens, or Pocket speaks as alba    |
        | /listen, /mute     | the mic opens or closes                            |
        | /undo 2            | the open program's last two moves are undone       |
        | /layout right      | the terminal docks right (none: the next one round)|
        | /cache clear frames| what void caches, or clears one                    |
        | /clear             | the transcript and the brain's memory are wiped    |

    Scenario: Everything else
      When I type or say anything else, e.g. "make the cube blue and the title bigger"
      Then the brain answers in a few sentences, spoken when I spoke, and works in the open space's React +
        TypeScript project with two tools: shell (bash over the space folder, with `raea` doing the work: cut, add,
        brand, render, check, still, media) and look (stills and contact sheets sent back as images)
      And the stage redraws after every change
      And the procedure it works from is prompts/sops/ts.md (rrl the language and its tools went on 2026-10-17)

    Scenario: Which model
      Then only two models are ever used (the founder, 2026-10-18): GLM 5.3 Flash on OpenRouter (~/.raea/openrouter.env),
        the primary model, for the agent and every writer, in Anthropic Messages, streamed; and clef-omni on Cloudflare,
        the decision model, for every judge, reviewer, check and scorer
      And without the OpenRouter key a turn fails and says so: there is no other provider to fall back to;
        RAEA_VOID_MODEL may name another GLM on OpenRouter (z-ai/glm-...), never another model
      And the system prompt, tools and history are marked for prompt caching (a second round reads ~8k tokens cached
        against a few hundred new)

  Rule: Every turn is traced, and its cost is known
    Scenario: Traces
      Then each turn is an agent.turn span with a chat span per model call (tokens from message_delta, cache reads and
        writes, cost as OpenRouter bills it, else at the model's rates) and an execute_tool span per tool, sent as OTLP protobuf to Arize Phoenix
        (localhost:6006, project "void", evals "void-evals") and written to ~/.raea-void/traces/<date>.jsonl in UGC
        Pro's shape (raea/agent/otel)

  Rule: An agent can drive it
    Scenario: The MCP server
      Then void serves MCP on 127.0.0.1:7457: state (thinking, speaking, listening), screenshot, send, act, eval, wait,
        logs, hear and say
      And a driver checks state before restarting void, since the person may be mid-conversation

  Rule: Evals run headless
    Scenario: void eval
      When I run `void eval [files or folders] --jobs 3 --run name`
      Then each scenario in evals/scenarios (Gherkin: what the person says, hidden expects on the render's size and length, what a vision judge must
        see at which frames, a cost cap) runs in its own folder with its own spaces, never the person's
      And results land in evals/results/<run>.jsonl with pass per medium, cost, time, tokens and cache rate, a trace id
        per scenario, and the change since the last run; dated numbers go in evals/results.feature

  Rule: Talk to it: the voice is four models on ONNX Runtime, with the host app's turn rules
    The rules and numbers are ~/host/docs/voice-mode.feature's, measured there on the phone; here they run on the
    mic's own clock (samples heard), so a recording replays them exactly (src/voice/turns.rs, with tests).

    Scenario: The models
      Then every model is an ONNX export run by ort =2.0.0-rc.13, the pin and prebuilt runtime of raea/meaning/embed
        and raea/perception, downloaded once into ~/.cache/raea-void/models (RAEA_VOID_MODELS moves it):
        | model                   | does                                                    | file          |
        | Silero VAD v5           | speech probability of each 32 ms chunk at 16 kHz        | voice/vad.rs  |
        | Smart Turn v3           | at a pause, is the thought finished (last 8 s, log-mel) | voice/turn.rs |
        | Parakeet TDT 0.6B v3    | the utterance's words, punctuated                       | voice/stt.rs  |
        | Pocket TTS (Kyutai)     | the reply, streamed in 80 ms frames at 24 kHz           | voice/tts.rs  |

    Scenario: Nothing to press
      Given the orb is on (click it, /listen, or the voice sheet)
      Then ~100 ms of speech starts a turn and a ~200 ms pause is judged: Parakeet hears the words so far and Smart
        Turn scores the pause
      And the turn is sent at once when Smart Turn says finished (≥ 0.5), after 0.8 s when unsure, after 1.4 s when the
        last word is one a thought doesn't end on ("about", "and", "the", "to" …), and after 2 s of silence whatever
        was said
      And without Smart Turn a pause of 0.85 s ends the turn, or 0.5 s after ".", "?" or "!"

    Scenario: Talking over it
      Given void is speaking
      Then its own voice never interrupts it: the echo it makes at the mic is learned while it talks
      And through headphones ~160 ms of speech at 0.7 or more stops it; through a speaker that bleeds, ~300 ms clearly
        louder than its echo
      And stopping drops the rest of the reply and cancels the brain's turn; what I'm saying becomes the next turn

    Scenario: The reply is heard as it's written
      Then reply text goes to Pocket a clause at a time as it streams (at . ! ? ; : or a newline, or a comma after four
        words), and each clause's audio plays as its frames decode
