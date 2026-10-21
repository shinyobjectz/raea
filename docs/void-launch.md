# void's launch and edit path: where the time goes, and the restructure (lightspeed F5)

F5.void_launch is the time from spawning void to the stage's first painted frame of the launch's object (the recent
workspace's first object). Target 100 ms; 206-238 ms on 2026-10-19 (startup.py, headless, under load 3-6). This note
is the measured timeline and the step that would take it under 100 ms. That step changes how void's window and
chrome come up, so it waits on raea-01's agreement; nothing here is built yet except where it says so.

## The timeline today (RAEA_STARTUP_TRACE, headless, titles on the stage)

| mark | ms | what |
|---|---|---|
| void main | 11-15 | process up |
| void window launching | 15-19 | dioxus-native's launch called (main thread) |
| audio device warm | 66-98 | CoreAudio's start, on its own thread (not on the path) |
| void app mounted | 107-148 | window, chrome surface, VirtualDom built and mounted: ~100-130 ms of the main thread |
| (native stage open) | = mount | app.rs opens the native stage, the stage thread starts and opens the object |
| workspace: proj | mount + ~60 | the launch's `enter`, a task spawned at mount, runs after the chrome's first render |
| native stage: rect | mount + ~80 | the chrome lays out the stage's box: its size reaches the stage, which makes its surface |
| void stage first paint | 196-225 | frame 0 painted (its sound placed after the paint, 56cc5ac) |

The stage's own open (bundle from cache, JS runtime, settings, bake from cache) is 63-75 ms and runs on its thread
while the chrome renders; since 56cc5ac it is no longer on the critical path. The path is: the window and the chrome's
mount, then the chrome's first render, then the workspace entering and the stage's box being laid out, then the paint.

## Measured no: the stage opened before the window (Phase A)

The stage thread started in main() before dioxus-native's launch, opening the launch's object at once and getting the
window's layer when the chrome mounted (patch kept outside the tree). Its open finished at ~92 ms, before the mount, but
first paint stayed where it was, since the paint waits for the stage's box (rect), which comes after the workspace
enters: paired, same build, RAEA_VOID_EARLY_STAGE on/off, 226/230/253 against 211/220/228 ms (load 3-5). The early open
competes with the window's creation for cores and buys nothing while the chrome's first render gates the box. Worth
taking again with the step below.

## Measured no: the workspace entered before the chrome's first render (was step 1)

`enter` runs in `spawn(async { ... })` from a use_hook. Entered in the hook itself (prototype, kept outside the tree),
the workspace enters at the mount (127 ms) but the stage's box still arrives ~79 ms later: the chrome's own first
style, layout and render is what the box waits for, not the task. With Phase A as well: first paint 215-222 ms, as
before. So the chrome's first render (~80 ms) and its mount (~100-130 ms) are the whole remaining path.

## The step (dioxus-native + stage_native, raea-01): the window and the stage before the chrome

The chrome's mount (~100-130 ms of the main thread) and its first render (~80 ms) are what's left. The window, its layer and the stage's
surface can come first, with frame 0 painted into the stage's box at the window's restored size, and the chrome mounted
over it after. Then Phase A pays: the object opens (~65-75 ms) while the window comes up (~20-30 ms for winit and the
layer), first paint at ~90-100 ms. Visible change: the stage shows a frame before the chrome's controls draw (for one
or two frames); raea-01's call whether that's acceptable or the chrome's first frame waits for it.

## The edit path: bundling (void's edit to swap, 132-163 ms)

apps/void/bench/video.sh: an edit re-baked 132-163 ms from the write: bundle 38-41 ms, bake 23-29 ms (ec41abe), swap 1-2
ms, the rest the watcher and the stage's pickup. An uncached bundle is 35-39 ms: bun's start ~7 ms, the rest the work
folder (the edit's folder, sdk/motion/src at 416 KB and the lite runtime copied each time) and Bun.build itself.

- A bundler kept running per void session (a bun process taking requests on stdin): no process start, and the compile
  plugin's output (react/js/lite/compile.ts) kept by file content, so motion's ~30 modules compile once.
- Resolution in place (bundle.ts's aliases and tsconfig paths pointed at the sources) instead of copying the work folder.
- Expected 36-41 -> ~10-15 ms. Not built yet; owner an agent session when lightspeed schedules it.

## Blocked below the steps

T3's bake is JS-bound on the system JavaScriptCore (~50 us a frame on titles where bun's JSC runs the same bundle at
~13; e0452c6), and each JS runtime opens in ~8 ms (context 3.2, prelude 0.9, bundle eval 2.6, first render). Logged
beside Z4 in docs/lightspeed.feature.
