---
title: "Inspect compositions and components"
summary: "See what each piece is made of on a project's Compositions page, find what a component reaches, and pick any part of a picture by name."
order: 5
sources: ["apps/app/src/compositions", "apps/app/src/detail/blocks/Toolbar.tsx", "apps/app/src/detail/Rail.tsx", "docs/organize.md"]
tutorial: "inspect"
---

Everything in raea is made from code, so you can always look inside it. This tutorial opens a project's Compositions
page, finds which compositions a component reaches, and picks parts of a render by name.

## Everything is code

Every piece in raea is a composition: a React file, written in TypeScript, that raea renders frame by frame on your
computer. Because it's code, you can always see what a piece is made of, and change any part of it.

## The Compositions page

Open a project's folder, and choose Compositions. Each row is one composition, with its size, its frame rate and its
length, and its earlier versions folded underneath it. The row also says what it used: the footage, sound and models
it drew, and the components it imports.

## Components

Switch to Components. A component is a file the compositions import, such as an end card or a title style. Each one
lists the compositions that use it, so you know what a change will reach before you make it.

## Pick a part of the picture

Open a render in the feed, and choose Select. Now click anything in the picture. raea knows which part of the scene drew
it, and names it, so a can is Yuzu Noon, not a cluster of pixels. The panel beside the picture lists every part, and
the picked one's properties.

## Edit and ask

Open a composition from its row to edit it. The picture is on top, with the timeline under it, and its layers,
properties and versions on the right. Move or scale what you've picked, and saving writes the next version. Or ask your
agent: when you say this, or here, it reads what you've picked, and makes the change in the same version you're
editing.

Next: [Publish](publish.md).
