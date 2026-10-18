Feature: One editing agent, two hosts: raea's harness and the host app's Agent
  Status 2026-10-14: design, approved by the user ("whatever you design for the agent i approve"). Nothing has moved
  yet; the phases below say what moves when and how each step is checked.

  The founder, 2026-10-14: all video editing work should move to raea; the the host app Agent stays (it is specific to that
  app and is how editing gets tested today), but raea should have its own harness agent, without the two drifting,
  and the host app's API is in the loop.

  Background:
    Given how things stand on 2026-10-14
      | piece                         | where                                   | size         | what it is                                                                 |
      | the loop                      | ~/host/agent/crates/pi-agent          | 2.0k lines   | pi's agent runtime, vendored Rust port (MIT): streaming loop, tool calls, permissions |
      | the providers                 | ~/host/agent/crates/pi-ai             | 3.2k lines   | Anthropic, OpenAI, Gemini, any OpenAI-compatible endpoint                 |
      | the host app's Agent               | ~/host/agent/crates/ugc-agent         | 38k lines    | pi's loop + 30 tools + sessions + the C interface for Swift               |
      | the evals                     | ~/host/agent/crates/ugc-eval          | 7.1k lines   | real MiniMax M3 through your raea API, real projects and renders, graded    |
      | Gherkin for the harness       | ~/host/agent/crates/ugc-spec          | 1.0k lines   | Features/Rules/Scenarios shared by ugc-eval and the prompt build          |
      | the embedder                  | ~/host/agent/crates/ugc-embed         | 0.7k lines   | SigLIP 2 on ONNX; raea/meaning depends on it (the wrong way round)        |
      | raea's edit verbs             | raea/rrl (agent.rs, memory.rs, recall.rs) | 4.3k lines | apply/undo/schema/at over an .rrl edit, guarded, with memory              |
      | raea's overnight loop         | raea/rrl/examples/overnight             |              | ugc-eval (the host app's real Agent) driving rrl edits, scored by hidden expects |
    And host already depends on raea by path (ugc-agent and ugc-core use ../../../../raea/engine), and
      ~/host/framework is a symlink to raea/engine, so "the host app uses raea" is the existing direction

  Rule: Dependencies point one way: the host app depends on raea, never the reverse
    Scenario: What raea owns after the move
      Then raea owns everything an editing agent needs that isn't about the host app the business:
        | moves to raea                  | as                                  | why                                                          |
        | pi-ai, pi-agent                | raea/agent/pi-ai, raea/agent/pi-agent | a generic loop and providers; both hosts need them           |
        | ugc-spec                       | raea/agent/spec                     | Gherkin for rules and evals; raea's docs are Gherkin too     |
        | ugc-embed                      | raea/meaning/embed                  | the meaning plane's model; removes raea -> host            |
        | the editing tools              | raea/agent/tools (crate raea-agent) | cut, edit, footage, look, music, performance, scene, review, versions, brief, read, write, and the shell's probe/lint/snapshot/render |
        | the editing prompt parts       | raea/agent/prompts/editing.feature  | the house editing rules, with their scenarios as evals       |
        | the editing eval scenarios     | raea/agent/evals                    | one bank both hosts are graded on (the overnight bank joins it) |
      And the host app keeps what is about the host app: sessions and the Swift interface (ffi.rs), the business tools (advice,
        brainstorm, clients, inbox, inspo, invoices, rate_card, references, site, tasks, web_fetch, web_search,
        canvas, watch, plan, script_check), Kelly's knowledge, the persona prompts, ugc-runner and the app-only evals
      And a tool whose side is unclear when its turn comes is decided by one test: does it need a the host app account,
        project store or business record? If not, it is editing and moves

  Rule: Two hosts compose the same pieces, so there is nothing to drift
    Scenario: The two hosts
      Then the host app's Agent is pi's loop + raea_agent::editing_tools(ctx) + its own tools + its prompts, as today
      And raea's harness is a binary, `raea agent`, that is pi's loop + raea_agent::editing_tools(ctx) + a project
        folder + raea's editing prompt; no the host app code at all
      And the model is a pi-ai provider either way: your raea API (the the host app proxy, MiniMax M3 today) is one
        provider config (base URL + token), direct Anthropic/OpenAI/OpenRouter keys are others; the harness reads
        RAEA_AGENT_PROVIDER, so using the the host app API from raea is a setting, not a dependency

    Scenario: Checks that catch drift
      Then a contract test in raea snapshots the editing tools' names, descriptions and JSON schemas, and the host app's
        build runs the same test against its composed tool list: a tool changed in one host fails the other's build
      And the editing eval bank runs against both hosts in the overnight loop; the same scenario failing on one host
        and passing on the other is reported as drift, not as a regression
      And the editing prompt's markdown is generated from raea/agent/prompts/editing.feature and included by UGC
        Pro's prompt build, byte-stable (the chat_is_todays_base_md test keeps passing)

  Rule: The move is staged; each phase ends with both hosts building and their evals unchanged
    Scenario: Phase 1 done (2026-10-15)
      Given ugc-embed moved to raea/meaning/embed (its version, edition and licence written out, no longer inherited
        from host's workspace), raea/meaning depending on it by `path = "embed"`, and ~/host/agent/crates/ugc-embed a
        symlink to it that is no longer a workspace member (no crate there depends on it)
      Then raea-meaning builds against it and writes a byte-identical SigLIP column (broll-screen.mov at 8 a second,
        against the column ingest stored before the move)
      And `cargo check -p ugc-core -p ugc-eval` in ~/host/agent is green without it; raea no longer depends on host
      And host's Cargo.lock drops the crate the next time anyone builds (left uncommitted here: another session had
        uncommitted changes in it)

    Scenario: raea's names are UGC-agnostic (2026-10-15)
      Given the user: raea will cover gaming as well as video, so its libraries don't carry the host app's name
      Then ugc-embed is raea-embed (package, lib raea_embed, binary raea-embed; a ugc-embed binary stays as an alias for
        the host app's knowledge scripts), and the model folder is ~/.local/share/raea-models (RAEA_MODELS, else UGC_MODELS;
        ~/.local/share/ugc-models is a symlink to it)
      And the SigLIP column it writes is byte-identical to before (broll-screen.mov at 8 a second)
      And the editing agent's crate is raea-agent (this file's phases); the host app's agent keeps its own name, being UGC
        Pro's; perception's Core ML packages still come from ~/host/models (UGC_MODELS), the apps' own folder

    Scenario: Phases
      Then phase 1: ugc-embed moves to raea/meaning/embed; host's Cargo paths point at it; raea/meaning and
        host's ad cards build and `ugc-embed check` passes (cosine 0.99 on 200 frames)
      And phase 2: pi-ai, pi-agent and ugc-spec move to raea/agent; host's workspace members become path
        dependencies; `cargo test` green in both; the xcframework builds; one eval round unchanged
      And phase 3: the editing tools move one at a time (footage, then cut, look, scene, music, performance, review,
        versions, brief, edit), each with its tests, behind raea_agent::editing_tools; the contract test lands with
        the first one
      And phase 4: `raea agent` runs the editing eval bank end to end on your raea API and on one direct provider;
        the overnight loop grades both hosts
      And every phase is a commit in each repo, and nothing is deleted from host until its replacement is green

    Scenario: Phase 2 done (2026-10-15)
      Given pi-ai, pi-agent and ugc-spec copied from host's HEAD into raea/agent/crates (ugc-spec renamed raea-spec;
        nothing depended on it), their own workspace with host's workspace.package and dependencies; the pi port's
        NOTICE.md and licence beside them; host's ugc-agent and ugc-eval depending on them by path
        (../../../../raea/agent/crates), its workspace members down to ugc-agent, ugc-core, ugc-eval and ugc-runner, and
        ugc-spec's lock entry gone (the lockfile kept at format 3)
      When it was built and tested in a host worktree first (the shared target, so nothing rebuilt from scratch),
        against the same tests run in the main checkout without the move
      Then raea/agent: `cargo test` passes but raea-spec's every-feature-parses test, which reads the host's Gherkin
        (RAEA_SPEC_REPO, else ~/host; raea's own docs wrap their steps) and fails on host's
        docs/new-project-flow.feature as ugc-spec's did before the move
      And host: the same with and without it (ugc-agent 228 and its integration tests, ugc-eval 23, ugc-runner 4 pass;
        renderer.rs lint_and_probe_in_process fails in both, the scene gate refusing its render)
      And HostCore's iPhone and Simulator slices build (1 min 51 s, 2 min 34 s)
      And an eval round on the moved crates passes: app-management-01-due-and-status 5/5, settings-01-accent-colour
        4/4, teaching-01-what-is-a-hook 3/3; pi-ai and pi-agent are byte-identical to host's, so the agent can't
        behave differently
      And so host's crates/pi-ai, pi-agent and ugc-spec are deleted (host 751a4ce, raea's copy first)
