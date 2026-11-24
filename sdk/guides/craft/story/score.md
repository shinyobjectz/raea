# Scoring a story

A story that passes its checks holds together. Whether it's good is a different question, answered four ways.

## 0. Read a draft back, then check it

The best use, measured: a draft script read back into story data (only what it shows), checked, and fixed by beat.
On eight prompts it tied a hard editing pass and beat the plain draft 26 of 32 judge votes; it came first where
there's most to track (a 40-shot film, a whodunit's fair-play clues). Treat the findings as a list for the rewrite,
not a score: reading a script back is noisy (a story that passes its own checks still reads back with about one
finding a beat).

## 1. Read it through lenses

`readThrough(story, [LENSES.mckee, LENSES.pas], telling)` gives each lens's map (which beats are which stage), its
findings and its proposals: what's missing, as a beat to add (`{ stage, after, onScreen, feel, why }`). A proposal is
a starting point for a beat, not a beat. Read [router](router.md) to pick the lens.

## 2. Ask typed questions

`questionsAt(story, telling, k)` (or `asker(story, telling)` for many beats) gives one beat's window as a state and
twenty typed questions about that beat, in the request format of decision models like Clef and Jev: `noul` (yes or
no), `choice` (one of named options) and `score` (an ordered scale). Each comes with its exact answer from the checks,
the systems and the lenses: does this beat break cause, which event does it show, which way does it turn, does it
continue the last shot, does the audience learn something, how tense is it, which McKee, Pixar and Campbell stage it is.

The window is story lines (`as: "lines"`) or a plain script (`as: "script"`). Hand the script form to raea cloud's
`judge` (Clef) to read a script that has no story data yet, and compare its answers with the exact ones on a story
that has. Measured 11-22 on 300 windows: Clef named the event a beat shows 86% of the time and its turn 74%, but
couldn't see a break in the story (AUC 0.54 to 0.69): the checks are the judge of structure, Clef of what's on screen.

## 3. Hold it to a real shape

The `launch` lens holds a story to what 156 real launch pairs measured ([launch](launch.md)), and a fingerprint of
real films that did well (raea fingerprint) says what their shape was: how fast the problem arrives, when the product
is first seen, how long the proof holds. A story's telling can be scored against it before
anything is drawn. Shape is evidence, not a rule: hold it loosely, and never copy a film's beats.

## What none of these can tell you

Whether it's funny, whether the line lands, whether the person will like it. Show the script to the person before
the film, and believe what they say over any score.
