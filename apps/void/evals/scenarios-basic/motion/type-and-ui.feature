@medium:motion
Feature: Kinetic type, counters and small UI animations
  Short pieces from an empty space: words on the beat, a number that counts, a button that changes state, a timeline.

  @level:easy
  Scenario: One word per beat
    Given an empty space
    When the person says "make 2 seconds of kinetic type, vertical: 'Make. Every. Frame. Count.' one word at a time, each word big and centred for half a second"
    Then expect edit.frames >= 55 and edit.frames <= 65
    And the judge sees "one big centred word per frame, reading Make, then Every, then Frame, then Count" at frames 7, 22, 37, 52
    And it costs under $0.10

  @level:medium
  Scenario: A stat that counts up
    Given an empty space
    When the person says "make a 3-second stat callout on a purple #6c3cff background: a big number counts up from 0 to 10,000 over the first two seconds and then holds, with the label 'creators paid' under it"
    Then expect hue(C.bg) >= 245 and hue(C.bg) <= 265 if camera C
    And expect edit.frames >= 85 and edit.frames <= 95
    And the judge sees "a purple frame with a big number and the label 'creators paid' beneath: near zero first, a middle value next, 10,000 at the end" at frames 2, 30, 85
    And it costs under $0.25

  @level:medium
  Scenario: A subscribe button
    Given an empty space
    When the person says "make a 4-second subscribe animation: a red rounded button that says 'Subscribe' pops in at the centre, a small white cursor slides onto it, then it turns into a grey button that says 'Subscribed'"
    Then expect edit.frames >= 115 and edit.frames <= 125
    And expect count(box) >= 2
    And the judge sees "a centred rounded button: a red 'Subscribe' button early, a cursor on it mid-way, a grey 'Subscribed' button at the end" at frames 20, 60, 115
    And it costs under $0.25

  @level:hard
  Scenario: A company timeline
    Given an empty space
    When the person says "make a 6-second timeline at 1920x1080: a horizontal line across the middle, then five dots pop in left to right, one per second, each labelled with a year from 2020 to 2024 and one word: Idea, Team, Launch, Growth, Exit"
    Then expect edit.width = 1920 and edit.height = 1080
    And expect edit.frames >= 175 and edit.frames <= 185
    And expect count(box) >= 6
    And the judge sees "a horizontal timeline line with dots and year labels appearing left to right: one or two early, about three mid-way, all five (2020 to 2024 with Idea, Team, Launch, Growth, Exit) at the end, labels not overlapping" at frames 40, 100, 178
    And it costs under $0.50
