---
title: "Connect your agent"
summary: "Bring the AI you already pay for over ACP, pick the primary that runs your workspace, and make agents of your own when you want them."
order: 2
sources: ["apps/app/src/plugins", "apps/app/src/settings/Agents.tsx", "apps/app/src/agent/AgentEditor.tsx", "apps/app/src/acp", "apps/app/src/chat"]
tutorial: "connect-your-agent"
---

raea runs the agent you already use. This tutorial connects one, picks the primary that runs your workspace, and
shows how to make agents of your own when you want them.

## How raea uses your agent

raea doesn't sell AI. It runs the agent you already pay for, on your own computer, through the Agent Client Protocol,
called ACP for short. ACP is an open standard for talking to coding agents. raea starts your agent, hands it the
project's brief and raea's own tools, and shows its work in the feed. Your agent signs in with its own account, and raea
never sees or keeps that login.

## Pick your agent in Plugins

Open Plugins in the sidebar. The Agents section lists the agents raea knows: Claude, ChatGPT through Codex, Kimi,
Gemini, and OpenCode, which reaches any model on OpenRouter. Pick the one you pay for and press Connect. raea installs
it if this computer doesn't have it yet, and opens its sign-in in your browser. Once it's signed in, raea sends it one
short prompt, to check that it answers.

## Bring your own

If your agent isn't listed, choose Your own agent, and give raea the command that starts it. Any agent that speaks ACP
over standard input and output will work. raea starts it in the project's folder, so it reads and writes the project's
files directly.

## The primary

Now open Settings, then Agents. The first row is the primary: the AI that runs your workspace. It answers in New chat
when no project is picked, takes what you write in the Inbox and on the Controllers page, and starts any work you don't
hand to someone else. Pick any AI you've connected and one of its models, Kimi until you change it. That's all the
primary is. raea doesn't add a director or a cast of agents on top.

## Choosing in New chat

Under New chat's box, one menu says what the chat runs on. The AIs signed in on this computer come first, each with
its models, and your agents come after them. Type to search. What you pick is for that chat only; the next one starts
on the primary again.

## Agents of your own

raea ships no agents. When you want one, press New agent. An agent is one of your AIs with more on top: a name you can
mention with @, the model it runs, instructions in plain words, the tools it may use, and what it may do without
asking. It's kept in your workspace's folder, so it's still there tomorrow, and every project can call on it. You can
even make one the primary, if you want a lead that works your way.

## Agents in a terminal

You can also work from your coding agent's own terminal. Install raea's skill with raea skills install, naming your
agent. The skill teaches it raea's loop: read the project, write the composition, check it, look at stills, then render.
The agents raea starts get the same skill, plus the feed's own tools.

```sh
raea skills install --agent codex     # or kimi, gemini; Claude Code gets it with raea itself
raea skill                            # print the skill your installed raea ships
```

Next: [A tour of the app](tour.md).
