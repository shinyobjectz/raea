@spec
Feature: Feed tasks: the graph view's layering, the to-do list, and sessions started outside the app
  Shane, 2026-11-13: "note to ship with this next batch for release is that in the react graph view when i zoom in it
  ends up obstructing other buttons and gui outside of the stage and across the app, so we need to make sure layering
  works there and remove the agents button in the top right of feed and inbox for a checkbox and this is a editable
  todolist built to look minimal, skimmable and easy to read with the ability to click into a task to see more
  details. the other note to add is in the timeline if an agent is currently running even if i have initiated the
  session myself in terminal or an app and it is still claimed or working on it without being terminated, meaning its
  still open or the session is active, we should see it in the feed and see updates from it. this should help tie
  into the todolist stuff too."

  Three parts. Each is done when it works in raea's window (not only in Chrome) and has its tests.

  Background:
    Given the app's code is apps/app (raea-feed); its branch for parts 2 and 3 is feed-tasks, part 1 is on its main
    And a window check is a headless raea window of the session's own: `node blitz/pw-run.mjs SPEC` on the BLITZ_E2E
      bundle, RAEA_PAGE_HOST_BIN a copy of the page host, a fresh raea.root each run

  Rule: FT1. The graph view stays in its own box
    # Status 11-13: built, in 0.1.3. Engine 37846080a (vendor/blitz-dom/src/node/node.rs, sdk/ui/src/dom/input.ts, a unit test
    # in sdk/ui/test/dom.test.tsx); app raea-feed #11 (AskCanvas clipped at its box, e2e/canvas-layer.spec.ts).
    # Cause: Chrome never showed it. In raea's window the canvas was DRAWN clipped, but neither hit test honoured an
    # overflow clip: blitz-dom's hit_inner tested a clipping box's children wherever they overflowed to, and sdk/ui's
    # hit dropped the inherited clip at a child with its own z-index and replaced it at a child that clips (a card is
    # overflow-hidden). Both halves are needed: with only sdk/ui fixed, elementFromPoint is right and a real press
    # still lands on the canvas (the host's target wins). Checked: the spec in Chrome and in the window; the app's
    # whole window smoke 28 passed, 49 failed, 4 skipped of 81 before and after, the same tests; sdk/ui 60 tests.
    # Not read by the hit tests: a transform on a box between a z-indexed child and its stacking-context root (the
    # painter's clips don't read it either); in sdk/ui, an absolute box whose containing block is above the clipper.

    @done
    Scenario: FT1a. A press outside the canvas never reaches it, at any zoom
      Given an ask with four pictures open on its canvas (React Flow), zoomed all the way in (scale 2)
      Then no point outside the canvas's box finds a piece of the canvas (document.elementFromPoint)
      And a press on the sidebar, the workspace picker or the header goes to what is drawn there
      And the canvas clips what it draws at its own box

    @done
    Scenario: FT1b. A box that clips takes no pointer for what it cuts away, anywhere in the window
      Given a box with overflow other than visible, and a child that overflows it, with or without its own z-index
      Then a point outside the box's padding box never hits the child or anything in it
      But a fixed box, and a box whose containing blocks are above the clipping box, are not cut by it

  Rule: FT2. The agents button is a checkbox that opens the to-do list
    # Status 11-13: built, in 0.1.3: raea-feed #12, merged ab68f0d (3b2353b, a2dc439). src/feed/todo.ts (the rows),
    # src/feed/TodoPanel.tsx, brief/format.ts removeTask and moveTask. The Agents panel and its menu surface are gone.
    # Checked: e2e/todo.spec.ts 4 of 4 in Chrome and 4 of 4 in the window; vitest (todo.test.ts, format.test.ts).
    # Decisions made here, each open to Shane's word:
    #  - It is the tasks the app already has, not a second list: the project's brief's tasks (## Tasks in brief.md:
    #    what the composer, the Board and agents write) and the edits sent on its items (comment tasks). Nothing is
    #    kept by the list.
    #  - Words are changed in a task's details (one click away, in the same column), not in the list's line: a click
    #    on a line opens it, so the line itself stays a single target.
    #  - A task's comments: an edit's replies; a brief task's are its linked item's open comments. A brief's task has
    #    nowhere of its own to keep a comment yet.
    #  - In the Inbox a new task goes to the project picked beside the line (the first, until picked).
    #  - Reorder is within a project's brief tasks; an edit keeps its place by when it was sent.
    # Left: no keyboard shortcut opens the list; the Board's card sheet still edits a task's agents, due date and tags.

    @done
    Scenario: FT2a. Where the agents button was, in a feed and in the Inbox
      Then a feed's header has no agents button; a checkbox there opens the to-do list in a column on the right
      And in a project's feed it is the project's list; in the Inbox the workspace's, each project's tasks under its name
      And its tooltip says what it is; the column closes from its own button and is as it was left next time

    @done
    Scenario: FT2b. Minimal, skimmable
      Then a task is one line: a checkbox, its words, and who has it when someone does
      And its handle and Remove show only on the line pointed at or focused
      And done tasks are folded away under a count ("2 done"), opened by a click

    @done
    Scenario: FT2c. Edited where it is read, written at once, with Undo
      When I type a line and press Enter, check a task off, change its words, drag it by its handle (or Alt+Up/Down),
        or remove it
      Then the change is written at once to where the task lives (the brief; a linked item's status; the comment)
      And Undo at the column's foot, or Cmd+Z, takes the last change back
      But a brief's change is undone only while the brief is still as that change left it

    @done
    Scenario: FT2d. A click opens a task's details; Esc goes back
      Then the details show its words, who is on it and what they last did, the feed item it is linked to, its comments
      And an agent at work on it in one of the app's own sessions can be stopped there, with its subagents listed
      And Esc goes back to the list, not out of the feed

  Rule: FT3. A session started outside the app is in the feed while it is open
    # Status 11-13: built, in 0.1.3: raea-feed #12, merged ab68f0d (7e90a7c, a2dc439). host/outside.ts (POST
    # /host/outside), src/feed/outside.ts, OutsideKeeper.tsx, OutsideBubble.tsx. No feed tool was added or changed.
    # Checked: e2e/outside.spec.ts 2 of 2 in Chrome and in the window (the host's list given by the spec);
    # host/outside.test.ts on files written as Claude Code writes them; the reader run against this Mac's six real
    # sessions. Not checked: a real session ending while the app's own window is watched by a person.
    # The signal, the smallest honest one: what the harness already keeps. Nothing new is asked of any session.
    #  - Claude Code keeps ~/.claude/sessions/<pid>.json for each open session ({pid, sessionId, cwd, name, status})
    #    and its transcript, <sessionId>.jsonl in its projects folder.
    #  - active: its file is there, its process is alive, and a person runs it (kind "interactive"). Holding a claim
    #    alone is not active: a task a session has that has ended still names it, without a live mark.
    #  - working: the file says "busy"; otherwise it is open and waiting at its prompt.
    #  - an update: its last words, the tool call at hand, its plan (the last TodoWrite), read from the transcript's
    #    end; an item it posts and a task it marks are already in the feed and on the task.
    #  - ended: the host no longer lists it.
    # Left: Codex, Gemini, Kimi and the rest keep no file that says a session is open, so they are in the feed only
    # by what they post; a session working in a folder that is no project's and that has posted nothing is in no
    # feed (Shane's own lanes in ~/raea are this case until they `raea show` something); rask claims and the lanes
    # board are not read.

    @done
    Scenario: FT3a. One bubble a session, in the projects it is in
      Given a Claude Code session open on this Mac
      Then it has one bubble in the feed of each project whose folder it works in or that it has posted to
        (its posts carry its inbox, claude-code-<the first 8 of its id>, as `raea show` names it), and in the Inbox
      And a session that is in no project's folder and has posted nothing is in no feed

    @done
    Scenario: FT3b. It reads as the app's own sessions do, without a Stop
      Then the bubble says its name and harness, that it is working (and for how long) or waiting at its prompt,
        the step at hand, its last words, and its plan
      And there is no Stop: a mark beside its status says where it runs and that it is stopped there

    @done
    Scenario: FT3c. Its updates come as it goes, and it ends when the session does
      When it says something new or starts another step
      Then the same bubble shows it within a few seconds, written only when what it shows has changed
      When the session is no longer open
      Then the bubble ends, its last words kept; a session that comes back opens it again

    @done
    Scenario: FT3d. It ties to the to-do list
      Then a brief's task that @names the session, and an edit on an item it posted, are its tasks
      And the bubble names them, and the to-do list shows the session on them with what it last did

    # Status 11-13 (FT3e, FT3f): built for 0.1.4 in raea-feed #25, stacked on #24; neither merged, both wait for Shane.
    # Shane 11-13, on the cards: "even if i started the chat or session out of raea app i can open up the chat in a
    # raea chat ui ... show what agent is working on what or any subagents and feel more like i can click to see more
    # and less rows of info". Checked: e2e/outside.spec.ts 3 of 3 in Chrome, with the host's answers given by the
    # spec; host/outside-chat.test.ts and host/outside.test.ts on files written as Claude Code writes them;
    # bridge-resume.test.ts on a test agent that offers fork. Not checked: a fork through the real claude-agent-acp.
    # Left: other harnesses' transcripts (only Claude Code's is read), and an image a prompt carried.
    Scenario: FT3e. Open chat opens it as a chat in raea
      When I press Open chat on its bubble
      Then its conversation so far opens as one of the workspace's chats: my prompts, its replies and thinking, its
        tool calls with their results and diffs, and its plan, read from its transcript (the newest 6 MB, saying so
        when older messages were left out)
      And a line at the top says where it was started and what writing there does
      And while it is still open where it runs, what it says next shows in the chat as it says it
      And opening it starts no agent
      When I write in that chat
      Then the agent carries it on in the folder it ran in: a copy of it (forked) while it is still open there, so the
        one there goes on by itself, or the session itself (resumed) when it has ended
      And from then on it is raea's chat, and following stops

    Scenario: FT3f. Its bubble says who is on what, folded until I open it
      Then folded, the bubble says what the session is on (its own title, or the name it was given), what it is
        doing now, and the subagents it has running
      When I click the bubble or More, which names what more there is
      Then it opens to its last message, its plan, its subagents (the running ones with the call each is on, then
        the latest done, with how many more are done) and the notes of mine it has
      And Less, or another click, folds it again
