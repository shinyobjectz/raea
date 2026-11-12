---
title: "Controllers"
summary: "A project's settings, which every agent reads before it works: change a value in place, describe a new controller in plain words, and see how agents post a decision to the feed."
order: 6
sources: ["apps/app/src/controllers", "apps/workspace/src/controllers.rs", "sdk/raea/src/brief.ts", "docs/projects.feature"]
tutorial: "controllers"
---

Controllers are how you tell every agent in a project how things should be done, once. This tutorial reads a
project's controllers, changes a value, and adds a new controller by describing it.

## What a controller is

A controller is a page of settings for a project, such as its delivery format, its brand, or how review works. Every
agent working in the project reads the controllers before it starts. So when you change a value, the next piece of work
follows it.

## The Controllers page

Open a project's folder and choose Controllers. Each controller is a card, with its fields in groups. A field can be a
toggle, a choice, a number, some text, a list, a set of agents, or a whole workflow. Change a value in place, and it's
saved at once, for the whole project.

## Make one in plain words

To add a controller, press New controller, and say what you want in plain words. Your workspace's primary builds it
from raea's field types, and it appears on the page, ready to adjust.

## Workflows

A workflow field lays out the steps a piece of work goes through, and who does each one: an agent, or you. When a step
fails, the workflow says which step the work goes back to.

## When an agent needs a decision

When an agent needs a decision that should hold for every piece from now on, it puts the controller in the feed, with
one line on what it needs. Set the value right there in the card, and every agent works to it.

## Controllers are files

Each controller is a TypeScript file in the workspace's controllers folder, and every change is a commit, by you or by
the agent that made it. So any agent, even one in a terminal, can read and change them with raea controller.

```sh
raea controller ls
raea controller set delivery.format.aspect-ratio 9:16 --by agent
raea controller write controllers/review.ts
```

Next: [Publish](publish.md).
