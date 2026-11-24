@spec
Feature: Telemetry: every raea we ship reports back, and only a source build stays silent
  Shane, 2026-11-20: "what can we do to repair and improve telemetry and start gettign more data back or betetr quality
  data back knwoing that the openign stratch did have lots of errros os we coudl partition form when the app was more
  stable, seperate amc/windows/linux users respectively as windows has had issues" and then "create a goal to do this
  and see if we can also get this from cli curl installs too not just desktop so that the onyl way to not get or send
  tleemerty data is cloning the github fi you want".

  Where it stands, read from D1 raea-accounts on 11-20 (app_events, onboarding_events; raea-api src/usage.ts, the app's
  apps/app/host/telemetry.ts):
  - Only the desktop app sends, and only when signed in. The CLI from `curl -fsSL https://raea.io/install | sh` or
    `irm https://raea.io/install.ps1 | iex` sends nothing but `raea bug` reports.
  - 109 people on 115 devices opened the app from 11-09 to 11-20. They made 722 agent turns (47 people) and 40 crashes
    (7 people). 465 accounts, 954 devices linked. Usage sharing is on for 161 onboarding rows and off for 210.
  - The first days are noise: about 90 `0.1.0-dev.1109.*` builds and roughly a dozen release candidates, nearly all
    ours.
  - Raw counts mislead. 165 of 289 Mac 0.1.7 turns failed, all from 3 people. Windows is the platform with trouble:
    `spawn npx ENOENT` crashed 6 Windows people on 0.1.7, and 0.1.8 fails 7 of 42 Windows turns. Of 22 Windows people
    who opened the app, 8 got a turn that didn't fail. On the Mac it's 31 of 81.
  - What's sent is thin. `features` is a timing tally of /host/* routes, not what people did. `render`, `publish` and
    `update` are accepted by the server and never sent. A turn says "failed" or nothing (successes carry stop: null).
  - It leaks. agent_turn's `agent` is the session id whole, with project slugs in it (`…tiktok-hq…`,
    `…driveeasier-tes…`), against the promise of no file names or project content.
  - Signed-out installs send nothing, so a crash before sign-in is invisible. That is where the Windows first run
    breaks.

  The rule this goal sets: every build raea ships (raea.app on every platform, the CLI from the curl or PowerShell
  installer, the cloud image) reports usage statistics and crash reports to raea-api. A build from a clone of the
  GitHub source has no reporting compiled in and sends nothing. That is the way to run raea without telemetry, and
  SHARING_NOTE already says so. Release builds have no environment variable or flag that turns reporting off.

  Two exceptions stay, because the law or a promise already made requires them (TM7):
  - In the EU, EEA, UK and Switzerland (usage.ts CONSENT), usage statistics need the person's consent, and opting out
    can't be sold. There, usage statistics stay off until turned on, for signed-in and anonymous installs alike.
  - Pro and Team were promised they can turn sharing off (SHARING_NOTE, raea.io/privacy). That promise holds until
    Shane changes the plans' terms. Changing it is his call, made on the pricing and privacy pages, not in code.

  Bars for every scenario:
  - Nothing sent names a prompt, file, path, project, media or a person's words. An identifier is a random install id
    or the account's id. raea's own stack frames are the only code text.
  - A finding is a rate per person, split by platform (mac, win, linux) and release period, never a raw event count.
  - The server re-checks everything the client claims: event names, sizes, consent, plan.
  - Each raea-api deploy notes its version and a D1 time-travel bookmark before its migration (as in
    agent-bug-reports), and has a test under apps/site/api/test.

  Rule: Data already collected can be read honestly

    @done
    Scenario: TM0. Every row knows its platform, channel and whether it's ours
      # Status 11-20: done and live. raea-api 788657a4 = private 3a45e44 (rollback e9fda6c0; D1 bookmark before 0050:
      # 0000090d-00000206-00005129-943914fd0b52536a6ef3c03839f4aa8a). 0051 applied; test/telemetry.sh. Read on production
      # after the backfill: Windows 0.1.7 6 of 15 people crashed; Mac 0.1.7's failed turns are 2 people outside raea.
      Given the plan
        | crate | file                                            | lines | job                                                                                   |
        | -     | apps/site/api/migrations/0051_app_events_parts.sql | 40 | app_events gets platform (mac, win, linux), channel (release, rc, dev, ci), internal (0/1), session and install (TM3, TM5); user_id may be null for an anonymous install; the table is rebuilt to drop NOT NULL; backfill from os, version and ADMIN_USERS |
        | -     | apps/site/api/src/usage.ts                      | +60   | platform and channel set from app.os and app.version when a row is stored; internal = an admin, an account named in INTERNAL_USERS (shane+ test accounts), a cloud worker's token (TM5) or channel dev/ci |
        | -     | apps/site/api/src/usage-report.ts               | 160   | GET /v1/admin/usage reports per platform x period x channel, release and not internal by default: people who opened, people with a turn that worked, people with a failed turn, people with a crash, each as a share of people |
      Then the backfilled table splits the 11-20 numbers above with no row left without a platform or a channel
      And a period is a named version range kept in one list in usage-report.ts: early (0.1.1-0.1.6), 0.1.7 on its own
        (the Windows npx crash, the Mac turn loop), stable (0.1.8 on); a new release joins stable unless it's split out
      And one person's 165 failed turns count as one person with failures

    @done
    Scenario: TM1. agent_turn names the kind of agent, never a session
      # Status 11-20: done. Live on raea-api 788657a4 with migration 0050_no_names.sql, which also turned up and fixed a
      # second leak: the features tally keyed requests by whole path, file names included (/host/look/<image>,
      # /host/asset/<file>); routes are now kept to their second slash (counts summed, totals unchanged), and 156 rows cut
      # mid-JSON were emptied. Production after: 19,457 rows, no slug or file name, all valid JSON. The app sends kinds
      # and routes from raea-feed #99 (2f702df), so from the next release.
      Given the plan
        | crate | file                         | lines | job                                                                                   |
        | -     | apps/app/host/acp/hub.ts     | +10   | agent_turn sends `agent` as the kind (claude, codex, gemini, kimi, opencode, antigravity, other), from the agent's command, not b.agent |
        | -     | apps/site/api/src/usage.ts   | +20   | any `agent` not in the known kinds is stored as its kind found by a word in it, else "other"; never the string sent |
        | -     | apps/site/api/migrations/0050_no_names.sql | 50 | rewrite every stored agent_turn's agent to its kind, and every features key to its route |
      Then no row in app_events holds a project or workspace slug
      And this lands first, before any other scenario

  Rule: What's sent can explain itself

    @done
    Scenario: TM2. A turn says how it ended and, when it failed, why
      # Status 11-20: done in raea-feed #99 (2f702df), ships with the next release. Every turn that worked was sent with
      # stop: null because bridge.prompt resolves to the stop reason itself and the hub read r.stopReason from it. An AI
      # that won't start for a chat is an agent_setup (part start), since no turn ran: the 0.1.7 case reads as
      # agent_setup {agent: claude, result: no_node, error: spawn_enoent, command: npx} (host/acp/hub-telemetry.test.ts).
      Given the plan
        | crate | file                       | lines | job                                                                                   |
        | -     | apps/app/host/acp/hub.ts   | +30   | stop: the ACP stopReason (end_turn, cancelled, max_tokens, refusal, ...), never null; on a failure, error: a class (spawn_enoent, auth, network, timeout, crashed, other) and the failed command's name only (npx, claude-code-acp) |
      Then a failed turn reads as agent_turn {agent: claude, stop: failed, error: <class>, command: <program>}
      And the 0.1.7 Windows crash, an AI that never started, reads as agent_setup {agent: claude, result: no_node,
        error: spawn_enoent, command: npx}

    @done
    Scenario: TM3. Crashes are caught wherever they happen, signed in or not
      # Status 11-20: done. Server: POST /v1/usage/anon live on raea-api 788657a4 (test/telemetry.sh). App: raea-feed #99
      # (2f702df). An engine crash is reported from the app's renders (telemetry.ts rendered, from make.ts and
      # reexport.ts), not a separate engine-exit.ts.
      Given the plan
        | crate | file                         | lines | job                                                                                   |
        | -     | apps/app/host/telemetry.ts   | +60   | unhandledRejection caught as well as uncaught exceptions; a random install id in ~/.raea/install-id (the onboarding funnel's install id where there is one); signed out, crash reports go with the install id; a session id per start on every event |
        | -     | apps/app/host/telemetry.ts   | +30   | a raea engine run the app started that ends with a panic or a signal: engine_crash {verb, code, signal, panic's first line scrubbed, its raea frames} |
        | -     | apps/site/api/src/usage.ts   | +40   | POST /v1/usage/anon {install, app, events}: crashes always (the app's crash policy), usage only outside consent countries; 5 batches an hour per address (day-hashed, never stored), 20,000 rows a day total |
      Then a Windows first run that crashes before sign-in is a row
      And a crash the host survives (a rejected promise) is a row

    @done
    Scenario: TM4. Events say what people did
      # Status 11-20: done in raea-feed #99 (2f702df); raea-api accepts the names (788657a4). As built: agent_setup in
      # onboard-ai.ts and hub.ts; milestones, render, update in telemetry.ts (called from server.ts, make.ts,
      # reexport.ts, update.ts, publish.ts); a publish event too; the page's item_opened and action through
      # src/lib/usage.ts from DetailView. An update is seen at the next start (the version changed), a failed one from
      # update.ts once per version and cause.
      Given the plan
        | crate | file                                | lines | job                                                                                   |
        | -     | apps/app/host/telemetry.ts          | +40   | the new names are allowed and sent: agent_setup, milestone, render, item_opened, action, update |
        | -     | apps/app/host/acp/setup.ts          | +15   | agent_setup {agent, result: ok, missing_cli, no_node, auth_failed, other, ms} each time an AI is picked or checked |
        | -     | apps/app/host/milestones.ts         | 60    | milestone {name: first_project, first_turn_ok, first_render_ok, first_export, first_publish, ms_since_install}, each once an install |
        | -     | apps/app/host/render-report.ts      | 60    | render {kind: video, 3d, game, still; frames; ms; ok; gpu backend; fps}; update {from, to, ok} |
        | -     | apps/app/src/lib/telemetry.ts       | 40    | item_opened {type: watch, scene, listen, game, page, ...} and action {name} from a fixed list of the main tools, through POST /host/telemetry |
        | -     | apps/site/api/src/usage.ts          | +10   | the same names accepted |
      Then the onboarding AI step's drop (216 reached it, 40 finished) splits into its causes by platform
      And time from install to first render that worked is a number per platform and period
      And `features` stays, read as performance only

  Rule: The CLI reports like the app

    Scenario: TM5. A CLI from the installer reports too
      Given the plan
        | crate | file                              | lines | job                                                                                   |
        | -     | apps/site/public/install          | +15   | writes ~/.raea/install-id (random, once) and says in one line that raea sends usage statistics and crash reports, linking raea.io/privacy |
        | -     | apps/site/public/install.ps1      | +15   | the same on Windows |
        | raea-io-telemetry (new) | io/telemetry/src/lib.rs | 160 | its own crate (raea-cli is near its 2,000 lines), a dependency only behind the `official` feature (TM6): a panic hook that writes the crash to ~/.raea/usage-queue.json; per verb run: cli_run {verb, ok, ms, exit code}; cli_open once a day with version, os, arch, chip, memory, cores, and whether the app is installed; sent in the background at most once a minute, never holding a command up; signed in, with the device token, else to /v1/usage/anon with the install id |
        | -     | engine/cli/src/native.ts          | +20   | the JS host's uncaught errors go to the same queue |
        | raea-cli | engine/cli/src/cli.rs          | +4    | under `official`: raea_io_telemetry's panic hook installed at start, and each verb's run handed to it |
        | -     | apps/site/api/src/usage.ts        | +20   | cli_open and cli_run accepted; channel ci when the client says CI, GITHUB_ACTIONS or another CI's variable is set; internal for raea's own cloud workers (sdk/cloud and Modal set RAEA_INTERNAL_WORKER, checked against a worker token) |
      Then a fresh curl install on a Mac, a Windows irm install and a Linux install each show up in the TM0 report
        within a minute of their first command
      And a command never waits on the network for telemetry: with raea-api unreachable, `raea --version` takes as long
        as it does today
      And an agent running raea headless in CI is counted as ci, not as a person

    Scenario: TM6. A build from source has no telemetry in it
      Given the plan
        | crate | file                              | lines | job                                                                                   |
        | -     | engine/cli/Cargo.toml             | +3    | an `official` feature, off by default; raea-io-telemetry and the endpoint are in the binary only under it |
        | -     | .github/workflows/release.yml     | +4    | release builds (every platform, and promoted builds) turn `official` on; tools/release-check.sh fails a release whose binaries lack it |
        | -     | tools/raea-app/                   | +10   | the app sets RAEA_APP_VERSION and telemetry only in a packaged official build; `RAEA_TELEMETRY=0` is honoured only when not official (our own dev hosts); `RAEA_TELEMETRY=1` in a dev host marks rows internal |
        | -     | engine/cli/tests/no_telemetry.rs  | 40    | a default `cargo build` binary contains no raea-api usage URL and makes no request while running a verb (checked with RAEA_API pointed at a local listener that fails the test on any connection) |
      Then `git clone` + `cargo build` gives a raea that sends nothing, with nothing to configure
      And a downloaded release has no switch that turns telemetry off, except TM7's

  Rule: Who may turn it off, and saying so

    Scenario: TM7. Off only where the law or a plan requires it, and said plainly
      Given the plan
        | crate | file                                   | lines | job                                                                                   |
        | -     | apps/site/api/src/usage.ts             | +20   | anonymous installs: usage refused from consent countries until the install opts in (`raea telemetry on`, stored by install id); crashes follow the app's crash policy; signed in, shared() as today |
        | raea-io-telemetry | io/telemetry/src/lib.rs | +30 | `raea telemetry` (raea-cli routes the verb to it) shows what's sent and the current choice; `on` and `off` work where the server allows (consent countries, Pro and Team) and otherwise say why not and how to build from source |
        | -     | apps/site/src/privacy/                 | +20   | the privacy page names the CLI and the cloud image as senders, what each sends, and building from source as the way to send none |
        | -     | apps/site/api/src/usage.ts             | +2    | SHARING_NOTE names the CLI as well as the app |
      Then a person in Germany with the CLI sends crash reports and no usage statistics until they run `raea telemetry on`
      And a Free person in the US with the CLI can't turn it off, and is told to build from source if they want none
      And a Pro person anywhere can turn it off from the app, the CLI or raea.io/account

  Rule: The data is looked at

    @done
    Scenario: TM8. A report and a release-day alarm
      # Status 11-20: done and live. admin.raea.io/usage (raea-admin deployed 11-20 21:48 UTC from private 3a45e44; the
      # usage_page tool makes its signed link; the daily page links it) and the hourly alarm in raea-api 788657a4
      # (usage-alarm.ts, usage_alarms). The line is 2 people, not 3: replayed on production's rows, 0.1.7 shipped 11-17
      # 22:44 UTC and by 11-18 12:00 two of four Windows people had crashed (spawn npx ENOENT); the third came 11-19 03:48.
      # Over 11-11 to 11-20, two people raise four alarms, each a real fault. 0.1.7 Windows and 0.1.3 Mac were marked
      # told at deploy (known already); 0.1.8 Windows' failed turns were left for the cron to tell.
      Given the plan
        | crate | file                                  | lines | job                                                                                   |
        | -     | apps/site/admin/src/usage-page.ts     | 150   | an admin.raea.io usage page on TM0's report: filters for platform, period, channel and internal; the onboarding funnel beside it; turn failures and crashes by error class |
        | -     | apps/site/api/src/usage-alarm.ts      | 80    | the hourly cron: when a release's share of people with a crash or a failed turn on one platform passes 10% with at least 2 people, the admin agent is told the release, platform and top error class |
      Then the 0.1.7 Windows npx crash would have raised the alarm on its first day (by 11-18 12:00 UTC, 2 of 4 people)
      And the page answers "is the newest release better than the last one on Windows" without writing SQL
