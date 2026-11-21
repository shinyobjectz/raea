@spec
Feature: Plans: a project's plan.org, kept by its agents
  Shane, 2026-11-14: "create a subagent that can flesh out the skills needed for tasks/plannig to be fully carried out
  by the agent right now its just blank and even if user doesnt touh it we need to have the againt use and maintain it
  and actualy use it for project managemtn and make that project managemnt system extensible with the task SDK. as a
  personal notel i liek or mode for managing projects so id liek to have that eb part of the base layer and make sure
  it integrates well with typescript/react/jsx correctly and the larger project in a way that is integral to the system"

  "or mode" is Emacs org-mode. A project's plan is one Org file, plan.org. It is the base layer: the app, raea's own
  agents, agents outside raea (Claude Code in a terminal) and people in Emacs all read and write the same plain text.
  The to-do list was blank because nothing wrote to it but the composer; now every agent plans in it, marks what it's
  doing and closes what it made, whether or not the person ever opens it. @raea/task (sdk/task) reads and writes the
  file without losing a byte, edits it by task id, lets a plugin add keywords, properties, kinds and views, and gives
  React its hooks and components.

  # Status 11-14: landed for 0.1.4. sdk/task (@raea/task) and this file in raea; the app in raea-feed (tasks-org merged
  # into integration-014 at 1214a21: vitest 242 files, 1509 tests, e2e todo, outside, messages, primary, heads,
  # consolidate 36 of 36); the skill in raea-skills #75 (skills/raea/plan.md).
  # Checked after the review: sdk/task 32 bun tests (org, edit, extend, tools), 35 with its react
  # tests under the app's vitest; the app's vitest 237 files, 1458 tests (base 229 files, 1403 tests, the same skips),
  # src/plan/agent.test.ts driving a turn's plan tools end to end on a faked host; tsc -b clean; e2e todo.spec.ts and
  # outside.spec.ts (7) and messages, primary, heads, consolidate (26) in Chrome; todo.spec.ts (4) and outside.spec.ts
  # (3) in raea's window (BLITZ_E2E bundle, node blitz/pw-run.mjs). The board (feed/Board.tsx, board/Lanes.tsx,
  # board/CardSheet.tsx), which no route mounted, was removed with PL8: the to-do list is the plan's one view. Not done: `raea project new` writes no plan.org yet
  # (Rust, engine/cli), so reference/files.md, generated from it, doesn't list one; the page makes it on the first
  # write. The brief page (BriefView, unmounted) still draws a brief's own ## Tasks. Compositions can't import
  # @raea/task until sdk/tsconfig.base.json and raea's bundler map it.
  # 11-16: PL7, agents finish what they plan (raea-feed plan-finish: plan/keeper.ts, PlanKeeper.tsx; sdk/task tools.ts).
  # Checked in raea's window: e2e chat-plan 2 of 2 with a real ACP stand-in planning through the tool port, and
  # chat-goal, respawn, todo, outside, heads, primary, consolidate, ai-login unchanged; vitest 1621; sdk/task 33.

  Background:
    Given the app's code is apps/app (raea-feed), the SDK sdk/task, the skill skills/raea/plan.md in raea-skills
    And the plan's tests: sdk/task/test/*.test.ts (PL2, PL3, PL5, PL6), apps/app src/plan/plan.test.ts (PL1c, PL4, PL4c,
      PL5), src/plan/agent.test.ts (PL5a, PL5b), host/plan.test.ts (PL4, PL5), src/feed/todo.test.ts and outside.test.ts
      (PL4b), e2e/todo.spec.ts (PL4b), apps/app src/plan/keeper.test.ts and e2e/chat-plan.spec.ts (PL7)

  Rule: PL1. A project's plan is plan.org at the top of its folder
    # Why there: it's the project's own file, versioned with it in the workspace's repo, found by anyone who opens the
    # folder (a person, Emacs, an agent in a terminal). Not under .raea/, which is raea's state, not the person's. One
    # file a project: the Inbox reads each project's in turn.

    @done
    Scenario: PL1a. The file says its title and raea's keywords
      Given a project with no plan.org
      When anything adds a task to it (the to-do list, the composer, an agent's plan_add)
      Then <project>/plan.org is made, starting "#+TITLE: <project's name>" and "#+TODO: TODO MAKING REVIEW | DONE DROPPED"
      And an Org reader (Emacs) cycles the same keywords: TODO, MAKING and REVIEW are open, DONE and DROPPED closed
      And a keyword written into a plan whose #+TODO: lines don't name it (MAKING in a file written by hand, a plugin's
        SHOT) adds its sequence's #+TODO: line to the head, so Emacs cycles it too; Org's own TODO and DONE need none

    @done
    Scenario: PL1b. The Org it uses
      Given a plan.org
      Then a headline with a keyword is a task; a headline with none is a heading: a stage that groups the tasks under it
      And a task may carry a priority ([#A]), tags (:look:sound:; a dash is written _), a planning line (SCHEDULED,
        DEADLINE, CLOSED), a PROPERTIES drawer, checkboxes (- [ ], - [X]), a progress cookie ([2/5] or [40%]) and links
      And its properties are :ID: (t- and eight letters and digits), :ITEM: (the feed item it stands on), :AGENT: (who
        has it), :FROM: (what asked for it), :KIND: and :VERB:; any other property is kept as written
      And links name feed items and tasks by id: [[item:i-door-2]], [[id:t-ab12cd34]]
      And a subtask is a task under a task; its parent's cookie counts its subtasks, or with none its checkboxes

    @done
    Scenario: PL1c. Everything is linked by id, never by name
      Given a task typed with "@grade #look due 11-20 → i-hall" (the to-do list, the composer)
      Then :AGENT: holds the agent's id; an @name no agent has yet is kept as @name, and shows that name
      And :ITEM: holds the item's id, the tags are the task's tags, and DEADLINE is that day this year
      And a task the composer writes for a message has :FROM: the message's id; one for a comment, ITEM#COMMENT

  Rule: PL2. @raea/task reads a plan without losing a byte
    @done
    Scenario: PL2a. A round trip changes nothing
      Given any text, Org or not: drawers it doesn't know, #+BEGIN blocks, tables, CRLF endings, tabs, no final newline
      When it's read and printed with no edits
      Then the text is the same, byte for byte
      And a planning line with a stamp it can't wholly read (a range of days, a habit's .+2d/3d) is still the planning
        line: the drawer under it, and its :ID:, are read, and the part it couldn't read is written back as it was

    @done
    Scenario: PL2b. A plan as values
      Given a plan.org
      Then readPlan gives every headline as an entry and the ones with a keyword as tasks, each with its state (todo,
        making, review, done, dropped, whatever its keyword is called), priority, tags and inherited tags, dates,
        properties, item, agents, from, parent and children, its stage, checkboxes, progress, links and body
      And a plan's own #+TODO: lines add keywords; a word a plugin defined keeps its plugin's state

    @done
    Scenario: PL2c. A changed entry is written again part by part
      Given an edit that changes one task's keyword
      Then only its headline line is written again; its drawer, body and every other entry are as they were
      And the headline keeps its own spacing and its tags end where they ended, as Org's org-todo leaves them; a
        planning line written again keeps its parts' order, with a new CLOSED first

  Rule: PL3. Every change is an edit by task id
    @done
    Scenario: PL3a. The edits
      Then applyEdits(text, edits) makes add, set, state, move, remove and check, in order, each naming its task by
        :ID: (or #N, its place among the headlines, for one that has no id yet)
      And add goes under a heading by its words (made when there's none), under a task (a subtask), or before or
        after a task; with none of them, at the end
      And every task is given an :ID: it lacks, and every progress cookie counts again

    @done
    Scenario: PL3b. Closing stamps CLOSED; reopening takes it off
      When a task's state becomes done or dropped
      Then its planning line gets CLOSED: [YYYY-MM-DD Day HH:MM], as Org's org-log-done does
      And opening it again removes CLOSED

    @done
    Scenario: PL3c. A wrong edit changes nothing
      When an edit names a task that isn't there (under: a task id included, which never becomes a stage heading), a
        day that isn't one, a keyword the plan doesn't know or a move into the task's own subtree
      Then applyEdits throws, saying what's wrong and how to find the right id, and the text is as it was

  Rule: PL4. The app keeps its tasks in the plan
    @done
    Scenario: PL4a. The host reads and writes plan.org whole
      Given POST /host/plan {op: read | apply | put, workspace, project}
      Then read answers the text and a stamp (same: true when the page's stamp still holds); a read for an agent
        (name: true: its turn, its plan_read) first gives each task written without an :ID: one, and writes it
      And apply makes the edits on the file (or on a new plan) and writes it through a temporary file, renamed over,
        answering with the text they were made on; edits naming a task by its place (#N) carry the text that place
        was counted in, and are refused when the file has changed since
      And put writes an older text back for Undo only while the file is still the text the page last saw
      And the e2e host (RAEA_FEED_KEEP=0) keeps nothing: the page's plan is the plan

    @done
    Scenario: PL4b. The to-do list, the composer and raea's own work write plan edits
      Given the to-do list (feed/TodoPanel.tsx) open on a project or the Inbox
      Then its rows are the plan's tasks as written (subtasks too) and the edits sent on the project's items
      And a task linked to an item stands where the item does (the work is the truth)
      And add, check off, reword, reorder and remove are plan edits made at once in the page and then on the file
      And Undo puts the text back only while the plan hasn't changed since ("The plan has changed since: nothing
        undone"), and puts back the file as the edit found it, so a task another writer added before the edit stays
      And the list reads the plan again every few seconds and on focus, so an edit in Emacs or a terminal shows; an
        agent's turn and its plan_read read it again first
      And a message sent in the composer is a task in the plan: after the last task carrying the view's tags, held by the
        agent it went to, :FROM: the message's id
      And raea's shaping, storyboards, directions and renders are its tasks under Plan and Make, each :ITEM: the card it
        waits on; a critique's gate is "<title> passes check and critique" with :ITEM: the newest critique
      And an item carries the tags of the tasks linked to it, a workflow field places a channel by its plan's tasks, and
        a session outside raea holds the tasks whose :AGENT: is @its-name or its inbox
      And the to-do list (feed/TodoPanel.tsx) reads and edits the plan the same way, by task id (the board that did
        too, never mounted, went with PL8)

    @done
    Scenario: PL4c. A brief's tasks move into the plan once
      Given a project whose brief.md still has ## Tasks (every brief before this, and the starter brief's "Write the plan")
      When the page has read its kept brief
      Then each task is added to the plan with its mark as its keyword, its ### stage as its heading, its @agents by id,
        its #tags, its due day as DEADLINE, its → item as :ITEM: and its craft verb as :VERB:
      And the task lines, the stages and an emptied ## Tasks leave the brief; everything else in it stays
      And a brief the plan can't take (the host refused the write) keeps its tasks, and is tried again next load
      And each task's id comes from its stage, its words and which of the same words it is, so a brief moved twice (two
        pages open on it, a reload before the brief was saved) adds nothing the second time

    @done
    Scenario: PL4d. Comment tasks stay where they are
      Given an edit sent on an item (model/tasks.ts, raea show --working/--done)
      Then it is not copied into the plan; it shows in the same to-do list, under the plan's tasks, as it did

  Rule: PL5. Agents use and maintain the plan, whether the person opens it or not
    @done
    Scenario: PL5a. The plan tools
      Given an agent the raea app launches (the feed MCP server, apps/app host/acp/feed-mcp.ts)
      Then it has plan_read (the open tasks by id; full: the whole file), plan_add (tasks under a stage or a task,
        held by the agent unless it names others), plan_update (mark, tick or add a checkbox, change fields, move,
        remove) and plan_close (DONE with CLOSED and the item it made, or dropped)
      And the page answers them through its plans (@raea/task answerPlanTool), a headless run (host/acp/memory.ts) on
        the folder's plan.org; a session with no project has no plan, and says so

    @done
    Scenario: PL5b. Every turn carries the plan
      Given an agent's turn in a project (src/acp/session.ts prompt)
      Then the prompt carries raea://plan/<project>: the rule to keep the plan, the task this message is (by :FROM:),
        and the open tasks by id under their stages (planDigest)
      And the rule: plan work of more than one step first, mark a task MAKING as you start it and REVIEW when it waits
        on the person, close it as you finish with the item it made, add what's left, drop rather than remove
      And every task it names has an id: a task written without one is given one before the plan is read for the turn
      And an agent launched without the plan tools is told to keep plan.org as text; a message whose task is already
        closed says so, rather than asking for it to be marked

    Scenario: PL5c. An agent outside raea keeps the same file
      Given Claude Code in a terminal in the project's folder, with the raea skill
      Then it opens plan.org (skills/raea/plan.md), plans and marks there as plain Org text, keeping each :ID:
      And the person sees its changes in the to-do list within a few seconds

  Rule: PL6. A plugin extends plans; React and raea's window read them
    @done
    Scenario: PL6a. Keywords, properties, kinds and views
      Then defineKeywords adds a sequence ("SHOT LIT GRADED | APPROVED") with each word's state; defineProperty a
        property with a check; definePlanKind a kind (its keywords, required properties, a check, defaults an add
        starts with; a new task of the kind starts at its sequence's first word); defineView a view any page can
        build (view(plan, name)); createRegistry one of a plugin's own
      And checkPlan says what's wrong by those rules: ids used twice, failing properties, unknown kinds, missing
        properties, keywords outside a kind's sequence
      And board, agenda and outline are the views raea ships

    @done
    Scenario: PL6b. React
      Then usePlan(text or source) gives the plan, its text and edit; useView a registered view; TaskLine, PlanOutline,
        Agenda and BoardLanes draw them, with toggled for a checkbox's edit
      And none of it uses a dynamic import or dangerouslySetInnerHTML, so raea's window bundle (one IIFE) holds it

  Rule: PL7. An agent finishes what it planned
    # 11-20 (PL8): the keeper's asks (PL7a, PL7b, PL7c, PL7f) are gone. Closing follows the work instead (PL8c), and the
    # task per message and the starter task that filled plans with tasks nobody closed are gone too. PL7d and PL7e stay.
    # Shane, 2026-11-16: "i notice often that it makes tasks but never completes them itself". His plans that morning:
    # TarotChess's agent posted a first pass and left its own follow-ups TODO (frame rate, a clipping score, a dark
    # encode); Cadence's said "all six masters are finished and posted" over a plan still saying Track MAKING and Build,
    # Render and Masters TODO (it had woken itself on a render's report, a turn raea sends no plan with); v0.1.5's
    # closed five tasks with every checkbox unticked, so the list read 0/4 on finished work; and its last turn ended
    # with Film v2's tasks MAKING, "waiting on three helpers", when the session stopped under them. Nothing asked any
    # of them to go on: only a /goal does that, and only when the person sets one.

    @done
    Scenario: PL7a. Each time an agent stops with tasks of its own open, raea asks it to settle them
      Given a chat in a project whose agent's turn ends (apps/app src/plan/PlanKeeper.tsx, beside GoalKeeper)
      And tasks of its own are TODO or MAKING in the plan, read from the file first
      Then raea's next turn to it lists them by id ("[raea plan] Ask N: ...") and says how each is settled: closed with
        the item it made, done now and closed, REVIEW saying what it needs from the person, dropped with why, or a
        line starting "WAITING:" naming the job of its own that will report back
      And it asks again each time the agent stops while one is TODO or MAKING; REVIEW is the person's, and ends it
      And the rule each turn carries says the same: finish what you planned; before you stop, every task of yours is
        closed, REVIEW or waiting on a job; close what a job finished when it reports back in a turn you started

    @done
    Scenario: PL7b. The asks stop
      Then they stop when none of its tasks is TODO or MAKING, or pause, saying why, when the person presses Stop, its
        session closes or the turn fails, its reply has a WAITING: line, two asks in a row leave the plan as it was, or
        12 have gone since the person's last message
      And the person's next message starts them again from nothing, and Continue over the composer asks at once
      And a chat with an open /goal is the goal's: its checks keep it going, and the plan's asks wait until it's met

    @done
    Scenario: PL7c. Which tasks are the agent's
      Then a task is the chat's when its agent (or the chat) holds it; or nobody holds it and it stands on an item the
        chat posted (its :ITEM:, or a note's item in its :FROM:: an agent writing plan.org by hand leaves :AGENT: out);
        or nobody holds it and it's MAKING while no other agent holds open work
      And a task nobody holds otherwise is the person's (written in the to-do list or Emacs, moved from a brief): never
        pushed on whoever is chatting; one SCHEDULED for a later day waits for that day

    @done
    Scenario: PL7d. Done means its checkboxes and subtasks are done
      When an agent marks a task done (plan_close, or plan_update's state) while a checkbox under it is unticked or a
        subtask open, as Org's org-enforce-todo-dependencies refuses
      Then the tool refuses, naming each by its place or id, and nothing is written
      And plan_close with all: true ticks them and closes the subtasks with it; ticked in the same call, it closes
      And dropped closes whatever is under it as it is; the person's own check-off in the to-do list isn't refused

    @done
    Scenario: PL7e. A post names what's still open
      Given an agent's feed_post in a project
      Then its answer carries the agent's open tasks and asks it to close what the post finished, with the item, so a
        turn the agent started itself (a job reporting back carries no plan) still closes them

    @done
    Scenario: PL7f. Seen in the chat
      Then each ask is a small line of its own ("Finishing the plan (2): 3 open"), never the marker in the person's words
      And over the composer, while it asks or after it pauses with tasks open, one line: "Plan  3 of its tasks open",
        "Finishing · ask 2" with × to stop asking until the next message, or "Paused · Continue" with why on hover

  Rule: PL8. One way to keep work, and any tracker beside it
    # Shane, 2026-11-20: "we continually in the desktop app get issues with tasks and starting or adding tasks and it
    # never actually closes any out. so maybe we should align plan to be simpler", then "make sure the project management
    # structure is clear dry and we dont have competing methods", and "the planning api will need to be able to easily
    # sync and extend to whatever system enterprise clients use so we dont want to be too opinionated about planning as
    # long as its consistent and fixable by the agent itself too". Read 11-20 in his workspaces: plans kept by an agent in
    # a terminal close their tasks (gambit 82 of 133, driveeasier 13 of 13, castle-busters 11 of 13); plans made in the
    # app don't (ten new projects hold only the app's own "Write the plan", open; p-pong 0 of 5), with PL7's keeper
    # installed. Tasks were written in four places (brief.ts stages, the composer's task per message, the starter task,
    # the agents' plan tools) and closed in one: an agent remembering to. Agreed with raea cloud (11-20), whose
    # agents keep their projects in plan.org on @raea/task and start them from Asana, Linear, Jira and other boards.

    @done
    Scenario: PL8a. plan.org is the one list
      # core/brief (brief.rs, parts.rs, tests.rs), apps/workspace doctor.rs, the app's src/brief/spec.ts and format.ts
      Then a project's tasks are in its plan.org and nowhere else: brief.ts says what to make (core/brief's Brief has no
        stages or tasks), and a brief that still has them reads, its tasks unread, with `raea doctor` saying they
        belong in plan.org
      And the app's brief writer (src/brief/spec.ts) writes no stages, its starter brief has no ## Tasks, and a brief
        that has them moves them into the plan once (PL4c, as before)
      And core/brief's object plan, which was also called a plan, is `parts()` in NAME.parts.ts (`raea spec --as parts`)

    @done
    Scenario: PL8b. A message is a message, not a task
      # the app's src/plan/turn.ts and plan.test.ts; e2e/chat-plan.spec.ts in raea's window
      Then the composer writes no task for a message sent: an agent plans tasks when the work is more than its turn
        (the skill's plan.md), and the person adds their own in the to-do list
      And a new project's plan starts empty, with no "Write the plan" task

    @done
    Scenario: PL8c. Closing follows the work
      Given a task linked to an item (:ITEM:)
      When the person approves that item, or any version of it
      Then the task closes DONE, stamped CLOSED, by the person, with no agent asked
      And where an open task stands (making, waiting on the person) is read from its item and from a session at work
        on it, as the to-do list already shows it (plan/words.ts standing), never from a keyword an agent must remember
        to set
      And it's checked in the app: src/plan/approve.test.ts, and e2e/todo.spec.ts in raea's window (checking off a
        task on an item writes that task DONE)

    @done
    Scenario: PL8d. The keywords are the file's
      # sdk/task newPlan (test/trackers.test.ts); the app's src/plan/keywords.ts and host/plan.test.ts
      Then a plan's #+TODO: lines are its keywords and Org's | says which close a task; raea reads any sequence that way
      And a plan the app makes keeps TODO | DONE DROPPED; raea cloud's TODO MAKING REVIEW | DONE DROPPED and a client's
        own words read the same, and newPlan(title, keywords) takes the sequence its caller keeps

    @done
    Scenario: PL8e. A task kept in step with another tracker
      # sdk/task src/trackers.ts; test/trackers.test.ts (with a raea cloud plan.org that passes checkPlan unflagged).
      Then a task names its items in other trackers with one property, :LINK: linear:ENG-12 asana:1209876543 (each the
        tracker's own key, never a URL; one per system), and the head maps each system's statuses to keywords, one line
        a system: #+LINK_STATES: linear In Progress=MAKING; In Review=REVIEW; Done=DONE
      And keywordFor(plan, system, status) is a status coming in and statusFor(plan, system, keyword) the status written
        back (the first one listed for the keyword), so a sync needs a mapping, not code per client
      And trackersOf(task) and trackerStates(plan) read them for a sync or a trigger's match

    @done
    Scenario: PL8f. An agent can fix its own plan
      Then checkPlan says each problem with its fix: an id on two tasks, a :LINK: entry that isn't SYSTEM:ID or is a
        URL, a system named twice on one task, one tracker item on two tasks, a #+LINK_STATES: keyword the plan doesn't
        know, and a plugin's kinds and properties (PL6)
      And nothing else is required: no :ID:, no keyword on a heading, no property, so headings that group, hold notes or
        hold a cloud agent's settings are never flagged
