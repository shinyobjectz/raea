Feature: A project's record: objects, tasks, questions, notes and versions, as data (@raea/data/project.ts)
  The founder, 2026-10-17: "project management, giving the agent an SOP, committing work or saving work, and managing
  tests and when to present to the user ... agentic understanding of tasks and spaces". Then: "a better method is
  removing spaces entirely and having the cube in the top left of the stage for looking at different objects, which we
  can make for anything that is granted a visual render. The project is the folder you chose to work in, then clicking
  on the title of the chat would be changing sessions." And: "render and build things like games in responsive full
  frame, not everything is a video and in a 9x16".

  Before this, the agent's world ended with its turn: no plan survived it, renders overwrote each other, undo was per
  shell command, notes were gone, and "done" was a prompt rule (the gates ran in evals only). The record is kept beside
  the work: ROOT/.raea/project.jsonl for the project (an append-only event log, folded on read), and each object's
  OBJECT/.raea/versions.jsonl with its files and renders by sha256 in OBJECT/.raea/objects, the action log's own store.

  The user's decisions (2026-10-17): versions on the action log with each version's render kept; a version at every
  render; no strict rule on asking, but questions the person answers by choosing (options and "Other"), like an agent
  chat's; ready only when every check passes or a failure is waived in writing.

  Rule: The project is the folder; it holds objects; chats are sessions
    Scenario: Objects
      Then an object is anything that renders, in a folder of its own: objects/<slug>/ (an older workspace's spaces/),
        with space.json (its name and its kind: video, game, deck, document, music, motion, scene3d or still) and its React + TypeScript
        project; a lone folder with none in it (an eval's) is one object itself
      And the cube at the stage's top-left lists them, each with its kind, draft or ready, and its last version; one
        is on screen (`raea object open`), and the agent's shell works in its folder (`open_object`, `new_object`)
      And a new object is shaped by its kind: a game, deck, motion piece or 3D scene landscape 1920x1080 (a game at 60
        fps), a video or still vertical 1080x1920 at 30, unless the request says otherwise
    Scenario: Live objects
      Then a game, a deck, a document or a piece of music plays live in the stage, full frame (void/play: its edit.tsx and @raea/motion through
        react-dom in the window's webview, the person's keys and pointer as its input, `live: true` on each frame's
        input, its generated sounds from what its renders made), rebuilt when its sources change
      And one that lays out from useVideoConfig() is played at the stage's size (responsive); any other at its own
        size, scaled to fit
      And a video object shows its render (the stage as before)
    Scenario: Sessions
      Then each chat is a session: its transcript and the model's history in ROOT/.void/sessions/<id>.json, named from
        its first request; the chat's name in the terminal's header lists them (newest first, + for a new one)
      And objects aren't a chat's: any session opens, changes and renders any object

  Rule: A project knows itself
    Scenario: What the agent is told at the start of every turn
      Then `raea status` gives, in a few lines: the project's purpose, its objects (each one's kind, state, last version
        and check), the open tasks with their next step and any unanswered question, and the notes

  Rule: A request is a task
    Scenario: The task record
      Then a task holds the person's words, its kind (new object, change, quick edit, question), the object it's for,
        its acceptance (the brief's `!` rules and anything the person stated), its steps, its questions and answers, the
        versions it produced, and its state: planned, working, needs input, draft, ready, delivered
      And "go on", or a change asked tomorrow, resumes the task it belongs to
    Scenario: Questions the person answers by choosing
      Then a question has the words, two to four options (each a label and what it means), whether several may be
        chosen, and always "Other" for the person's own words; `raea ask` ends the agent's turn, the window shows it as
        a panel (numbered options, Other as a text field), and the answer goes to the record and comes back to the
        agent as the person's next message
    Scenario: Notes
      Then a note keeps what the person decided or prefers ("never orange", "Kelly signs off on claims") for the project

  Rule: Work is saved as versions
    Scenario: A version at every render
      Then each `raea render` saves a version of its object: its number, the message, the action log's position, every
        tracked file's hash (files written outside the shell, compiled edits and the person's own edits too), the
        render's hash (renders no longer overwrite each other), and each `raea check` puts its result on it
      And an unchanged object gives back its last version; a check of it is put on that version
    Scenario: Going back
      Then `raea versions` lists them, `raea diff A B` names what changed, `raea checkout N DIR` writes one beside (the
        cube's Open: an earlier version on the stage, the object untouched), and `raea restore N` puts the object back
        as version N, through the agent's shell (the cube's Restore too), so undo takes it back
      And a restore saves the object's current files as a version first when none holds them
    Scenario: What is tracked
      Then every file of the object but media/, assets/ (generated sound, a cache), .raea/, .score/, node_modules/, space.json, terminal.json and the
        rendered videos (kept by hash in the version instead)

  Rule: What has to pass, when (the check policy)
    Scenario: Draft and ready
      Then a piece is a draft until its last version's full check (`raea check --full`) passes with every acceptance
        rule of its task; it is ready when that holds, or when each failure is waived with a written reason
      And ready holds for the version it was given: a later version is a draft again until it's checked and marked
      And a check of nothing fails: the template's edit, or a render of a frame or two, is "nothing is built yet"
    Scenario: The gates, in the app as in evals
      Then a turn that changed an object ends with a full check (the end gate; the app too, since 2026-10-17)
      And a reply that calls an object ready while its record says draft is sent back once: run the full check and
        `raea task state ID ready`, or present it as a draft
      And an empty answer (no words, no tool: MiMo's rounds cut off at 4096 tokens) is sent back twice at most;
        MiMo writes up to 16384 a round
    Scenario: What a presentation carries
      Then what changed, the version, the check in a line, the assumptions made, and what the agent needs from the
        person, if anything

  Rule: Where it's tested
    Scenario: The tests
      Then data/conformance/project.test.ts (objects, questions, versions, restore, checkout, the check policy),
        cli/conformance/surface.test.ts (every forwarded command runs in project.ts), void's sessions and workspace
        tests, and evals/scenarios/project/project.feature (three multi-turn scenarios graded on the record)

  Rule: What a person can try (plain words)
    Scenario: In void
      Then ask for something ("make a game of pong with sound effects"): the agent writes it down as a task, plans its
        steps, makes the object, and it appears in the cube at the stage's top-left
      And when it needs your call, a panel opens over the stage with a few choices, each explained, and "Other" for your
        own words; pick one and it carries on
      And click the cube to switch objects: a game plays live (click the stage, then use the arrow keys), a video plays
        with sound; each row says what the object is, whether it's a draft or ready, and its latest version
      And click a row's version ("v8") to see them all: Open shows an earlier one without changing anything, Restore
        puts the object back as that version (the agent's undo can take it back)
      And an object is "ready" only when its full check passes, or each failure is waived with a written reason; until
        then it says "draft", whatever the agent says
      And click the chat's name in the terminal's header to switch chats or start a new one
