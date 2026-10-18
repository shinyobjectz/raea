@owner:editing @ticket:T-139
Feature: Render-speed budget
  How long raea may take to render a realistic edit before release 1.0 counts it as too slow
  (release/features.yaml render-speed). The yardstick is one bench edit, the kind of cut the in-app Agent makes most:
  a 30 s 1080x1920 talking head with word captions and one motion overlay. Each device class gets its own budget.

  The bench edit is framework/bench/render-speed/talking-head-30s.w.html. It plays the take seven eval scenarios use
  (takes/take-20261008-111906: the phone's HEVC 1920x1080, rotated, 32.2 s) from 1 s in, with its audio and the eval
  music bed (agent/evals/fixtures/camera-roll/music-bed.m4a) under it. The captions come from the take's words.json in
  the cut tool's style: up to six words a group, three text-shadows per word, an accent pill on the spoken word. A
  title opens it, and a GSAP lower-third card slides in from 11.5 s to 18 s as the motion overlay.

  The automated check is framework/bench/render-speed.robot (`tix verify framework/bench/render-speed.robot`),
  tagged feature:render-speed.

  Rule: On the Mac, the bench edit renders in 320 s or less
    The Mac budget was set on the Apple M4 Mac mini (10 cores, 16 GB) that the agents share. It is the slowest
    render seen (the first verify, 207.3 s at load 171) times 1.5, rounded up to the next 10 s. The machine was busy with other agents' builds while
    measuring (load average 27 to 201), so the budget is loose on purpose: it catches a renderer that regresses badly
    or hangs, not a 20% slip. Tighten it once a quiet-machine number exists.

    Scenario: The bench edit renders within the Mac budget
      Given the bench edit "framework/bench/render-speed/talking-head-30s.w.html"
      And the renderer the Mac eval host runs, "~/.cargo/bin/raea", at its defaults
      When it renders the edit to MP4 under `nice -n 10`
      Then the output is 1080x1920 and 30 s long, within 0.2 s
      And the wall time is within the Mac budget:
        | machine                 | budget | timeout (fails) | video length | budget as real-time factor |
        | Apple M4, 10 cores, Mac | 320 s  | 640 s           | 30 s         | 0.09x                      |

    Scenario: The measurements behind the Mac budget
      Given the bench edit and the Apple M4 Mac mini, shared with other agents' builds
      When it rendered three times on 2026-10-10 with `nice -n 10`
      Then the runs measured:
        | run | wall time | real-time factor | load average as it started | output                           |
        | 1   | 94.8 s    | 0.32x            | 27                         | 48.5 MB, 30.00 s, 1080x1920 H.264 |
        | 2   | 175.8 s   | 0.17x            | 201                        | 48.5 MB, 30.00 s, 1080x1920 H.264 |
        | 3   | 137.7 s   | 0.22x            | 137                        | 48.5 MB, 30.00 s, 1080x1920 H.264 |
      And the first verify of the suite took 207.3 s at load 171, slower than all three
      And the budget is 207.3 s times 1.5, which is 311 s, rounded up to 320 s
      And the real-time factor is video seconds per wall second, so above 1 is faster than real time
      And a fourth run under `/usr/bin/time -l` (122.6 s wall, load 152) used CPU time that the load barely changes:
        | measure                                  | value   |
        | user CPU, renderer and its ffmpeg        | 229.8 s |
        | system CPU                               | 36.4 s  |
        | CPU seconds per video second             | 8.9     |
        | peak resident memory, largest process    | 1.3 GB  |

  Rule: On an iPhone with the A15, the budget is still to be measured
    The A15 (iPhone 13 Pro Max) is the slowest iPhone the release aims at. framework/docs/gpu.feature records a
    different 20 s 1080x1920 edit rendering in 6.1 to 7.0 s on it (about 3x real time, 2 workers on the 2 P-cores),
    but nobody has rendered this bench edit on the phone yet, so there is no budget to hold it to.

    @todo
    Scenario: The bench edit renders within the A15 budget
      Given the bench edit, rendered by the app on the phone
      When it renders on an iPhone 13 Pro Max
      Then the wall time is within the A15 budget:
        | device            | chip | budget         | measured |
        | iPhone 13 Pro Max | A15  | to be measured | not yet  |
