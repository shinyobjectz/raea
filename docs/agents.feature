@spec
Feature: Agents in the raea app: chats run on a provider, agents are optional templates, and whoever posts an item hears back
  Shane, 2026-11-09: "i want to make sure if you upload a video you are the subscribing agent to recieve the hook bakc
  to the session. and lets delte all the agent types we made default or have on the system. that said what we could
  ahve is a way for an agent to self assign its role if intiialized outsid eof raea. and if assigned in raea it would
  be set automatically but i need a way to start a new chat with just athe provider i dont have to use an agent its
  just a template type of skills and prompts d things"

  What the app is today (apps/app, read 11-09):
    - Agents are workspace members (src/model/agents.ts). The fixtures ship eight (Boards, Grade, Score, Checker,
      Director, Builder, Playtester, Cutter), and the page makes a Director and a Critic in every workspace the first
      time it needs them (builtIn). New chat starts a session with the workspace's Director (src/chat/NewChat.tsx); the
      feed's composer files a card for @director.
    - An item posted by an agent in raea carries its session (item.connection). A person's note goes to that session
      as its next turn if the session is open in the page (src/agent/notes.ts promptSession), else only into its log.
      Comments on the picture wait for "Send N edits to <agent>" (docs/review.feature R1e).
    - A session outside raea (a Claude Code session, a script) posts by writing <project>/.raea/feed/<id>.json. It has
      no session raea can reach: the launch film's 25 comments (i-acp-mw9k336z-2) sat in its JSON until Shane told
      the session to read them. That item borrowed a fixture's connection (c-plan), so the app named the Director as
      its author.

  The model this file sets:
    1. A chat runs on a provider: an ACP agent from the registry (Kimi, Claude, Codex, Gemini, ...) or a command of
       the person's own. That is all a chat needs.
    2. An agent is an optional template on top of a provider: a name and role, instructions, skills, tools. The app
       and raea ship none; a person makes them.
    3. Whoever posts an item is its subscriber: what the person sends on it reaches that session, inside raea or out.
    4. A session states its own role. One started from a template in raea gets the template's role without asking;
       one started outside raea says who it is when it posts, and may change it.
    5. There's no Director. The workspace has one primary: the harness that runs at its root (its own chats) and
       whatever the person doesn't pick for. It's a provider on a model, or one of the person's agents; making an agent
       the primary is how someone gets an orchestrator, and it's still only that choice (A6, Shane 11-12).

  Background:
    Given the app is apps/app (raea-feed): src/chat, src/agent, src/model/agents.ts, host/acp, host/feed-items.ts
    And the starter projects (RaeaStarters) and the e2e fixtures are sample content, not the person's agents

  Rule: A1. A new chat needs only a provider
    # Status 11-09: built (raea-feed 601d4ae). New chat's "Runs on" menu: PROVIDERS from the ACP plugins, Kimi first,
    # each marked as ai-find answers when the menu opens, then the workspace's templates; the pick is settings.chatOn.
    # A provider is an Agent with `provider` (model/agents.ts providerAgent, id p-<registry>-<workspace>), never in
    # agentsIn; every New chat session gets raea's context and tools (src/chat/useChat.ts). e2e "New chat runs on a
    # provider with no agent".
    Scenario: A1a. Starting a chat with just a provider
      When I open New chat
      Then I choose what runs it: a provider (Kimi first, then the registry's agents signed in on this Mac) or one of
        the workspace's agent templates, if there are any
      When I pick a provider and write
      Then a session starts on that provider in the project picked under the box, with raea's context and no
        template's instructions, and its page names the provider

    Scenario: A1b. A template is a choice, never a requirement
      Given a workspace with no agents
      Then New chat, the feed's composer and a project's chat all work, each on the workspace's primary

    Scenario: A1c. One menu picks the AI, its model or an agent
      # Shane 11-12: "when I go to composer and I select a model, I could pick a model provider that I have, only show
      # the ones that are connected ... groups with those model providers at the top of the dropdown, and then we
      # could scroll down and look at our agents or type to search and select an agent"
      # Status 11-12: built (raea-feed primary-agents: src/chat/RunsOn.tsx; e2e "New chat runs on a provider",
      # "an agent is a provider on a model").
      When I open what a chat runs on, under New chat's box
      Then the AIs signed in on this Mac come first, Kimi first, each a group of its models; one that isn't signed in
        or isn't on this Mac isn't offered (Settings → Plugins connects it)
      And the person's agents come under them, each with its square mark and what it runs on
      And typing searches both, and Enter picks the first match
      And the pick is that chat's: the next New chat starts on the primary again

  Rule: A2. raea ships no agent types
    # Status 11-12: the dev page's sample agents (Planner, Builder, Cutter, ...) are authors only too: they keep their
    # recorded sessions for the e2e suite, and no list offers them (raea-feed primary-agents, model/data.ts).
    # Status 11-09: built (raea-feed 601d4ae). No Director or Critic is made; the planner and critique post as raea
    # (raeaAgent, provider "raea"), tasks for it say @raea and an old brief's @director reads as raea. The built app's
    # sample world keeps its agents as `by` authors only (model/data.ts sampleAuthors); the dev page keeps them as
    # templates for the e2e suite. The host's model prompts (host/director.ts, compose.ts) still cast the planner as a
    # film director: that's the craft role, not an agent. e2e "raea ships no agents".
    Scenario: A2a. A new workspace has no agents
      Given a new install, or a workspace made in the app
      Then Settings → Agents and the sidebar's Agents list nothing until the person makes one, and no Director or
        Critic is made behind their back
    Scenario: A2b. What the built-ins did still happens
      Then the feed's composer starts a chat on the default provider in its project (and files the card), and the
        critique of a render is posted by raea itself, not by an agent named Critic
    Scenario: A2c. Sample content keeps its authors as names
      Given a starter project's items, made by the fixtures' agents
      Then they show their author's name and colour as written in the item, with no agent behind it

  Rule: A3. Whoever posts an item hears what's sent on it
    # Status 11-09: A3b built (raea-feed aee0ca2: Item.by, host/inbox.ts, POST /host/inbox; e2e "an outside poster's
    # item is theirs"). One line per send: {item, project, title, kind: note|edits, text, pin?, comments?: [{id, text,
    # kind, pin, at}], when}. A3a's "opened to take them" and A3c are next.

    Scenario: A3a. A session in raea
      Given an item posted with feed_post from a session in raea
      When I send notes or edits on it
      Then they reach that session as its next turn; a session that isn't open is opened to take them

    Scenario: A3b. A session outside raea subscribes when it posts
      Given a session outside raea posts an item, with `by` in its JSON: {"name", "role"?, "inbox"}
      Then the item shows `by.name` as its author, and the bar's send says "Send N edits to <name>"
      When I send them
      Then each is appended as one JSON line ({item, project, comment, text, pin, part?, kind, when}) to
        ~/.raea/inbox/<inbox>.jsonl, and marked sent on the item
      And the session hears it by following that file (`tail -n0 -F`, or its harness's monitor): that is its hook

    Scenario: A3c. An item no one subscribed to says so
      Given an item with neither a session nor `by.inbox`
      Then the bar says the edits have no one to go to, instead of a send that goes nowhere

  Rule: A4. A session states its role
    Scenario: A4a. Started from a template in raea
      Then the session's role is the template's, set when it opens, without asking
    Scenario: A4b. Started outside raea
      Then it states its role in `by.role` when it posts, and a later post may change it; the app shows the newest

  Rule: A5. What a session shows the person comes up in the raea app, as something they can use
    # Shane 11-09: "my agent working raea keeps making sheets in preview and not in raea how do we make it so when using
    # raea and raea app is installed new edits new sheets or examples or questions are pulled up in the raea app and
    # not preview. we should see them as interactive too. the whole point of raea is interactive artifacts and things
    # and recording them after making an answer."
    # Status 11-09: built. App side raea-feed 077e77f (host/show.ts, host-file.ts, raea:// links in src/App.tsx,
    # answers and results to the inbox); CLI engine/cli/src/show.ts; the skill's section 5 says it (raea-skills).
    # A5d/A5e 11-09: apps/check/src/hook.rs (raea hook AGENT), apps/tools/src/agent_hooks.rs (each harness's config).
    Scenario: A5a. raea show from any session
      Given a session outside raea (a terminal agent) with the raea app installed
      When it runs `raea show FILE...` (stills, a video, a sound, a NAME.script.ts or .md, a .tsx) or
        `raea show --question "Q" --choice A --choice B`
      Then the app's host makes the item those files are (several stills a storyboard, a .tsx an artifact, a project's
        composition its editor, a question an ask) in the project named or the one the session's folder is in, with
        the session as its poster (`by`), and the window comes up on it (raea://item/<project>/<id>)
      And when the app isn't running it's started first; when it isn't installed the files open in the Mac's own viewer
        and the command says nothing was posted

    Scenario: A5b. The person's answer is recorded and reaches the session
      When the person leaves notes or edits, picks an answer, or an artifact sends its result
      Then it's kept on the item and appended to the session's inbox (A3b), which `raea show` names when it shows

    Scenario: A5c. Agents show, they don't open
      Then the raea skill tells every agent to show work with `raea show` (or feed_post inside the app), never to open
        a render, a sheet or a question in Preview or a browser while the app is installed

    Scenario: A5d. A hook stops the open once, and a second try goes through
      # Shane 11-09: "yes lets do that or at least make it double confirm"
      Given raea's hooks are installed for the agent (`raea skills install --agent A`) and the raea app is on this Mac
      When the agent runs a shell command that opens a still, a video or a sound in the Mac's viewer or a browser
        (`open`, `open -a Preview`, `xdg-open`, `qlmanage -p`)
      Then `raea hook A` stops it, and the agent reads why: `raea show` with those files instead
      And the same command run again in that session within ten minutes goes through: the agent's second yes, for when
        the person asked for the viewer or raea show can't
      But `open -a raea`, Finder (`open -R`), links and files the app doesn't show (a PDF) are never stopped

    Scenario: A5e. Every harness, not only Claude Code
      # Shane 11-09: "make sure all hooks and usages are acp agnostic not just claude"
      Then `raea skills install` adds the same hooks for Claude Code (settings.json), Codex (hooks.json), Gemini CLI
        (settings.json, BeforeTool and AfterTool) and Kimi Code (config.toml [[hooks]]), and `raea hook` answers each
        in the shape it reads (a stop is exit 2 with the reason on stderr, which they all take)
      And the hooks hold inside the raea app too, which runs each ACP agent with its own shell and config
      And `raea show` names its poster from the harness that started it, found up its parent processes when the
        harness doesn't mark its shell, so Kimi, OpenCode and the rest post as themselves with an inbox per session

  Rule: A6. One primary, and agents are providers with more on top
    # Shane 11-12: "in our docs we say like there's a director and the director owns everything. That's not really
    # true. All we have is a primary agent harness that runs in the global root of the workspace ... I could assign an
    # agent to be that level, that orchestrator or that like director, but it's really just a primary harness choice
    # or model choice." And: "the ability to create an agent that is something beyond calling the model harness ...
    # those agents ... are in fact just the model provider with all the same bells and whistles".
    # Status 11-12: built (raea-feed primary-agents). The primary is settings.chatOn + chatModel (chat/provider.ts
    # primaryOf); Settings → Agents sets it. Agents are kept as agents/<id>.json in the workspace's folder
    # (host/agents.ts, POST /host/agents) with launch.model.

    Scenario: A6a. The primary runs the workspace
      Given Settings → Agents, whose first row is the primary, Kimi on its own default model until changed
      When I pick an AI's model or one of my agents there
      Then the workspace's own chats (New chat with no project, the Inbox's composer, the Controllers page's bar) run on
        it at the workspace's root, and so does every chat, feed message and task where nothing else is picked or named
      And no page calls it a Director: an agent made the primary is an orchestrator only because it's there

    Scenario: A6b. An agent is a provider on a model, with its own name, instructions and tools
      When I make an agent in Settings → Agents (New agent)
      Then I pick what it runs on (Kimi, Claude, ChatGPT, Gemini, OpenCode, or a command of my own) and, for a provider,
        one of its models, beside its name, colour, instructions, tools and approvals
      And it's kept in the workspace's folder as agents/<id>.json, so it's there after a restart and travels with the
        workspace's repo
      And chats on it run its model with its instructions, as a chat on the bare provider would with none

  Rule: A7. Each AI's own settings, for the AI and for an agent on it
    # Shane 11-12: "acp allows us to adjust provider/harness specific options?" and "selecting a harness for a custom
    # agent ... we would need the same settings and options once we select what we pick".
    # An ACP session reports its config options (session/new configOptions): each has a category (mode, model,
    # thought_level, model_config, or the AI's own) and is a select or a switch. Claude's: mode, model, effort, fast,
    # ultracode. Kimi's: model, thinking, mode. raea draws them as reported, so no AI needs code of its own.
    # Status 11-12: built (raea-feed primary-agents). Page: agent/AgentOptions.tsx, settings/AIOptions.tsx and the
    # agent editor; host: app-prefs providers (POST /host/onboard ai-prefs), host/acp/hub.ts optionsFor, bridge.ts
    # useOptions.

    Scenario: A7a. An AI's defaults on this Mac
      Given Settings → Agents, whose Your AIs lists every AI signed in on this Mac, each saying what's set for it
      When I open an AI's Options
      Then I see every setting it reports but its model, grouped as Thinking, Mode and More, each on Default, which names
        the AI's own value
      And I can give it environment variables (NAME=value) and extra arguments to start with
      And what I save is kept on this Mac and reaches every chat on that AI started afterwards: the primary, a chat
        picked in New chat, and an agent on it that doesn't set its own

    Scenario: A7b. An agent on an AI has the same settings
      When I pick what an agent runs on in its editor
      Then the same settings show for that AI, each on Default, which names the value I gave the AI ("Claude's: High")
        or else the AI's own
      And what I set is the agent's (launch.options, launch.env, launch.extraArgs), and picking another AI starts them
        over
      And a command of my own has no settings to show; only its environment is kept

    Scenario: A7c. Settings layer, and one that no longer applies says so
      Given the AI's own default, then my defaults for the AI, then the agent's settings, then the chat's model
      When a session starts
      Then each setting is the last of those that sets it, applied with session/set_config_option
      And one the AI no longer reports, or a value it refuses, is skipped with a notice in the chat, never a failure
      And raea changes no mode for me: Approve everything (Settings → General) already answers the AI's permission asks,
        so its own bypass or yolo mode, or a plan-only mode, is on only if I pick it
