Feature: void's shell: files on the left, the agent on the right, the space in the middle
  Status 2026-10-21: planned, not built. Handoff from the root-regroup session to the void app session.
  The founder, 2026-10-21: "a header that's constant in the void app ... we move the chat window specifically to the right
  panel so it's always on the right panel. And then the terminal is always at the bottom ... on the left we have a file
  tree ... now the agent comes from the right side and we can chat with it over there, manage our sessions and
  everything ... the magic terminal is like a space to manage, like think of like a open space for controls ... the
  stage or the canvas is the visual feedback preview."

  Today a space lays out the whole window (spaces/void: header, stage, terminal, player, sheets). This splits it:
  the shell is fixed and the same in every space (header, files, chat, the terminal dock); a space is what changes
  (the stage, the terminals it opens with, any panels it adds beside the stage). It builds on void.feature's rules
  "A space is a setup" and "Objects are where work lives", and on classic chrome's removal (a branch in review as this
  is written: spaces/void becomes the only UI; this shell grows out of it).

    ┌ ● ● ●  ▣ cube   [video: launch-ad] [game: pong] [build-3d: kitchen]   +    cat ┐
    │ FILES          │                                            │ CHAT             │
    │ ▸ footage      │                  STAGE                     │ sessions ▾       │
    │ ▸ compositions │      (video, 3D, a canvas, a flow graph)   │ the agent's      │
    │ ▸ music        │                                            │ thread           │
    │────────────────│                                            │                  │
    │ SPACES         ├────────────────────────────────────────────┤                  │
    │ ▣ video ▣ game │ TERMINAL  [timeline] [audio] [cli]  +   ⇅  │ prompt           │
    └────────────────┴────────────────────────────────────────────┴──────────────────┘

  Rule: The shell is the same in every space
    Scenario: The header
      Then a header runs across the window's top: the traffic lights at the left, then the cube, then the open tabs,
        then the cat at the right
      And the header is the window's title bar (it publishes { drag } as spaces/void's header does today)
      And the cube shows and hides the left panel (files, spaces, controls), as ⌘B does; a space is opened as a tab
        from that panel's Spaces section (Shane, 10-21)
      And the cat shows and hides the chat panel, as ⌘J does; settings move to the chat panel's menu (and a
        right-click, as now) (Shane, 10-21)

    Scenario: Files on the left, chat on the right
      Then the file panel is on the left and the chat panel on the right, each collapsible (⌘B files, ⌘J chat) and
        resizable, their widths and shown or hidden kept in ~/.raea-void/settings.json
      And a setting swaps them (chat left, files right); nothing else moves

    Scenario: The shell is TSX too: the whole window is raea (Shane, 10-21: "we want the entire app to also be raea")
      Then the header, file panel, chat panel and dock are React panels drawn by raea, shipped in apps/void/shell/
      And apps/void/shell/shell.tsx lays out the window itself (the header, the left panel, the chat, the dock, where
        the spaces and terminals go), as a space.tsx lays out a space: void's Rust places what it says and keeps only
        the plumbing (the window, the engine, input and focus, the pty, the agent), no UI of its own; void's own
        fallbacks (its row over the stage, player bar, sheets and terminal) go as P5 retires spaces/void
      And each region stays a page of its own while that's cheaper (settling, a crash kept to one region); one page
        for the window waits on the engine painting only what changed, and on a feature that needs it (a drag across
        regions)
      And a workspace may override a shell panel by name (shell/<name>.tsx in the workspace), the way a space of the
        same slug stands in for void's

  Rule: The file panel is the workspace, all of it
    Scenario: The tree
      Then its top section is a tree of the whole workspace folder (compositions/, media/, brand/, spaces/, controls/, ...),
        folders collapsible, with VS Code's Material Icon Theme (PKief, MIT: shipped whole, mapped by file and folder
        name, then extension, as the theme's manifest says)
      And it replaces the Files tab and its sheet: import (+, and drops anywhere on the window) lands files where
        void.feature's "The space's files" says (media/, models/, docs/, assets/), copied under 256 MB, linked over,
        transcribed, indexed for search by meaning, nothing uploaded
      And search over it is the Files tab's search: names and words at once, meaning a moment later
      And per file: open, show in Finder, rename, remove; folders: new file, new folder
      And it watches the folder, so files the agent writes appear as it writes them

    Scenario: Spaces, terminals and controllers are a second section
      Then under the tree a section lists the spaces, terminals and controllers there are (void's and the project's),
        each with its icon: a click on a space opens it as a header tab, on a terminal as a dock tab, on a controller in
        a terminal of its own; each goes to its tab instead if it's open
      And they are files (space.tsx, terminal.tsx, control.tsx), so forking one, the agent editing one, or copying one
        to another project is file work; the open one reloads in place when it changes

  Rule: Tabs are spaces; compositions show in them (Shane, 10-21)
    Scenario: Spaces are the header's tabs
      Then the header's tabs are the spaces open on the project, as many as I like (video, game, build-3d); a space is
        global to the open project, not tied to one composition
      And each space shows one composition on its stage and remembers which; going to its tab shows it as it was left

    Scenario: Compositions
      Then what I make (a video, a game, a 3D scene) is a composition: a folder in compositions/ whose TSX declares
        <Composition>s, the same thing raea-motion renders (Shane: the word video editors know; "object" means a mesh
        in 3D work, so it replaces void.feature's "objects" and object.json becomes composition.json)
      And compositions compose: one is placed in another as a React component (<Sequence from={90}><LaunchAd/>
        </Sequence>), and, to come, by name from the tree
      And a composition never depends on a space: any space that fits its kind may show it, two at once

    Scenario: Opening a composition
      When I open a composition from the tree (or /open, or the agent opens one)
      Then it shows on the current space's stage if that space fits its kind; otherwise it goes to an open space that
        does, or opens its kind's default space as a new tab (an edit: video; a game: game; a 3D scene: build-3d; set
        per kind in the workspace's settings, and per composition in composition.json "space")
      And a space never owns files: the tree shows the same workspace in every space; a space may only suggest an order
        (video puts footage first)
      And tabs close with ⌘W, cycle with ⌃Tab, and the open ones come back when the workspace opens again

  Rule: Chat is the agent, and it comes from the right
    Scenario: The chat panel
      Then it holds the agent's thread: what I type or say, its replies, its process lines (verb, details, ✓ or ⚠), its
        questions (the question sheet, inline), voice (the mic, heard-so-far dim, spoken words violet)
      And sessions are a list at its top (today's chats sheet): new, switch, rename, each tied to the workspace
      And the agent's tools still drive everything: it can open spaces as tabs, show compositions in them, open
        controls in the dock, and write compositions, spaces and controls

  Rule: Terminals are spaces for control; controllers are their children (Shane, 10-21)
    Scenario: Terminals
      Then a terminal is a kind of space: where a space lays out panels around a stage showing a composition, a
        terminal lays out controllers, with no stage (Shane: "controllers are a child of terminals which are different
        types of spaces that have control"); a dock tab is a terminal
      And a terminal is a folder with a terminal.tsx (@raea/terminal): <Terminal name> holding <Controller src> laid
        out with <Split> and <Stack>, e.g. edit: the timeline over audio; shell: a cli; 3D: the outliner beside a
        parameter sheet
      And a controller opened alone (from the tree, +, or the agent) comes in a terminal of its own, holding just it

    Scenario: Controllers
      Then a controller is a React panel in a folder with a control.tsx (with an optional icon.svg and AGENTS.md): a
        control surface for the stage (a timeline, an audio mixer, a node graph, a parameter sheet, the phone as a
        controller) or a shell (the cli) (Shane, 10-21: "controllers would be objects for terminals")
      And void ships the timeline, audio and cli controllers and the terminals made of them; a project's add to them,
        and one of the same slug replaces void's
      And a controller hears void and the stage (@raea/terminal's hooks: the player, the timeline's clips, what
        sounds, its pty) and acts on them; its exported actions are the agent's tools while it's open

  Rule: Spaces and terminals have SDKs of their own, on one core (Shane, 10-21)
    Scenario: Two SDKs
      Then @raea/space is for laying out work around a composition: <Space>, <Stage>, <Panel>, <Toolbar>, <Sheet>,
        and hooks on what's on the stage (what it publishes; picks and tools sent to it); it's today's @raea/void
        (apps/void/kit/space.ts), renamed
      And @raea/terminal is for control: <Terminal>, <Split>, <Stack>, <Controller>, and hooks for what a stage
        doesn't have: usePlayer (play, pause, seek), useTimeline (clips), useSounds, usePty, and to come usePhone (the
        phone as a controller: docs/platform.feature)
      And both are built on one core, the panel runtime (settling, voidIn, actions as the agent's tools, the theme and
        icons): a controller costs what a panel costs and acts the same, and a controller may sit in a space's panel
        when that's wanted
      And they live in sdk/space and sdk/terminal; shell.tsx places spaces and terminals in the window above both

    Scenario: The dock
      Then the dock attaches to one edge of the stage (bottom by default; top, left or right with ⇅ or /layout), between
        the file and chat panels, never outside them
      And it holds terminals as tabs, as many open as I like (two shells, an edit terminal); + opens any terminal or
        controller, and one may be open twice (each cli its own shell); the open set and the edge are the space tab's,
        kept with it
      And a space names the terminals it opens with and their edge (<Terminals open={["timeline", "audio"]}
        side="bottom" />); I can open others and close those

    Scenario: The cli terminal
      Then cli is as close to a real terminal as it can be (Shane, 10-21): the founder's login shell in the workspace
        folder, alacritty_terminal over a pty, full-screen programs, colours, the shell's own history, since the agent's
        transcript now lives in chat

  Rule: Only what's seen is drawn (Shane, 10-21: "we do need some kind of efficiency rendering system")
    Today every panel is a React page on V8 of its own, stepped and painted 60 times a second while it's shown, even
    when nothing on it changes; with many tabs and terminals open that cost grows with each one. The shell keeps it
    flat: what costs is what's on screen and changing, not what's open.

    Scenario: A panel draws when it changes
      Then a shown panel (shell, space or terminal) steps and paints only when something reaches it: a pointer or key,
        voidIn or the stage's published data changing, a timer or an animation frame it asked for, a file it reads
        changing; otherwise it isn't stepped and its last texture stays on screen
      And an idle window (nothing playing, nothing typed) costs close to no CPU and no GPU, however many tabs are open

    Scenario: Tabs out of sight sleep
      Then a tab out of sight is not stepped or painted: its stage pauses (a game or a playing video stops where it
        is, as a hidden window does) and its panels keep their state but get no frames
      And going back to it draws it on the next frame, from where it was
      And past a budget (open tabs' footprint, a setting with a default from the bench) the least recently seen tabs
        go cold: their pages are dropped and they reopen from their files and their kept state (the space's composition,
        open terminals, scroll, selection) when seen again
      And a dock terminal out of sight is the same, except a cli: its shell keeps running and its output lands in its
        scrollback, drawn when it's seen

    Scenario: Measured, not assumed
      Then apps/void/bench/shell.py opens 8 tabs and 6 terminals and records, headless: CPU and GPU time over 10 idle
        seconds, footprint(1) (not RSS), and the time to show a sleeping tab and a cold one
      And those numbers are the step's gate and go in the commit

  Rule: A space is the stage and its controls
    Scenario: What space.tsx says now
      Then <Space> keeps <Stage/>, <Terminals>, <Panel side> (a panel beside the stage, inside the middle: build-3d's
        outliner, vector's layers) and agent={{ files, skills }}
      And <Toolbar side="top" src="panels/header.tsx"> and <Terminal> are read as before during migration: a header
        panel becomes a toolbar over the stage (the shell owns the window's header), a <Terminal src> becomes a
        terminal opened in the dock
      And spaces/void is retired into apps/void/shell/; video, build-3d, typewriter and vector keep working, checked by their
        benches

  Rule: A project is a folder with a raea.json; what's in it is known by its marker files (Shane, 10-21: "some sort of
    rea project file so that we can map controllers and compositions ... a more freeform structure")
    Scenario: The project file
      Then a folder is a raea project when it holds raea.json, as package.json marks a Node project:
        { "raea": 1, "name": "Launch", "find": ["**"], "ignore": ["node_modules", "renders"],
          "spaces": { "video": "video", "game": "game", "3d": "build-3d" } }
      And it says only what a person would: the project's name, where to look and what to leave out, and the space each
        kind of composition opens in; every field but "raea" may be left out (find: everything, ignore: nothing beyond
        hidden folders and node_modules, spaces: void's defaults)
      And it's the person's, to commit; void's own state (chats, transcripts, open tabs, caches) is in .raea/, not
        hand-edited and git-ignored (it replaces .void/)

    Scenario: Things are known by their marker files, not where they are
      Then under what "find" covers, a folder holding composition.json is a composition, one holding control.tsx a
        controller, terminal.tsx a terminal, space.tsx a space; compositions/, controls/, terminals/ and spaces/ are
        where new ones go, by convention only (ads/q3/hero/ is as good as compositions/hero/)
      And what's particular to one is in its own folder (composition.json: its name, kind, entry file, its own
        default space), so copying a folder to another project carries all of it
      And the tree's second section lists the spaces and controls found, by these files

    Scenario: Projects from before
      Then a folder with .void/workspace.json reads as a project of that name, objects/<slug>/ and the old
        spaces/<slug>/ objects as compositions, space.json as composition.json; nothing is moved or renamed unless
        asked (/tidy), and raea.json is written the first time void saves anything to the project

  Rule: Built in steps, each checked headless
    Scenario: The order
      Then P1: the shell's header, the file panel's tree with the icon theme, and chat as the right panel (the
        transcript, sessions, the prompt with paste and history), checked by a apps/void/bench/shell.py through act click,
        act key and act panel_rect as space_void.py did, and shell panels drawing only when they change
      And P2: the terminal dock: raea.json and finding things by their marker files, controls as files (control.tsx
        anywhere the project finds), the cli and the timeline and audio controls, the four edges (done 10-21, aa855404:
        each dock tab one controller)
      And P2b: terminals as spaces for control (terminal.tsx laying out controllers; a dock tab a terminal; a lone
        controller in a terminal of its own), @raea/space and @raea/terminal on one core in sdk/, and shell.tsx laying out
        the window, checked by bench/dock.py with an edit terminal of two controllers (terminals and the two SDKs done
        10-21; the core is still @raea/motion's panel runtime, not a package of its own; shell.tsx next)
      And P3: tabs: spaces as header tabs, many open, compositions shown in them (each kind's default space), objects
        renamed compositions in void (composition.json, .void/ to .raea/, void.feature), tabs out of sight sleeping and
        going cold past the budget, gated by the bench's numbers
      And P4: the tree's import, search and file actions in place of the Files tab and sheet, which are then removed
      And P5: spaces migrated (Toolbar and Terminal read as above), spaces/void retired, void.feature's Rules updated
      And each step leaves void working; test voids run headless, never a window on Shane's screen

  Rule: Open questions for Shane
    Scenario: Not decided yet
      Then do spaces keep side panels beside the stage (assumed above), or does everything that isn't the stage become a
        terminal?
      And answered 10-21: cli is a real terminal; the cat shows and hides chat; the cube shows and hides the left panel
