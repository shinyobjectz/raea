@medium:ugc
Feature: Retention edits on the talking-head ad
  The things a creator adds to keep people watching: a progress bar, harder punch-ins on the jump cuts, motion on
  the b-roll.

  Background:
    Given the space "ugc"

  @level:medium
  Scenario: A progress bar
    When the person says "add a thin progress bar along the very bottom edge of the frame that fills from left to right over the whole video"
    Then expect count(box) >= 1
    And expect B.y >= 1820 if box B
    And expect min(box.w) <= 100 if frame = 10
    And expect min(box.w) >= 900 if frame = 730
    And the judge sees "a thin bar at the bottom edge of the frame, short near the start, about half the width in the middle, nearly the full width at the end" at frames 10, 370, 730
    And it costs under $0.25

  @level:easy
  Scenario: Harder punch-ins
    When the person says "the punch-ins on the jump cuts are too subtle. make every punched-in shot 1.3 and leave the wide ones as they are"
    Then expect C.zoom >= 1.28 and C.zoom <= 1.32 if clip C {sound: voice}, C.zoom > 1.05
    And expect count(clip) >= 8
    And the judge sees "the speaker is framed noticeably tighter in the second frame than in the first" at frames 60, 140
    And it costs under $0.10

  @level:easy
  Scenario: A slow push on the screen recording
    When the person says "give the screen-recording b-roll a slow push-in, from 1.0 to about 1.15 over the time it's on screen"
    Then expect C.zoom <= 1.03 if clip C {z: 1}, C.from < 400, frame = C.from
    And expect C.zoom >= 1.1 if clip C {z: 1}, C.from < 400, frame = C.to - 1
    And it costs under $0.10
