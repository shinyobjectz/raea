@medium:motion
Feature: Motion graphics from an empty space
  Short standalone pieces built from 2D rows (title, text, box) with keys and rules. Nothing draws rect or circle
  yet, so shapes are boxes.

  @level:easy
  Scenario: A logo sting
    Given an empty space
    When the person says "make a 3-second logo sting for my studio NOVA: the word NOVA scales up from nothing with a little overshoot, settles, and a thin line sweeps under it. dark background"
    Then expect edit.frames >= 85 and edit.frames <= 95
    And the judge sees "a dark frame where the word NOVA is small or absent at first, then large and settled with a line underneath by the end" at frames 2, 30, 85
    And it costs under $0.10

  @level:medium
  Scenario: An animated bar chart
    Given an empty space
    When the person says "make a 4-second animated bar chart at 1920x1080 of monthly revenue: Jan 12k, Feb 18k, Mar 25k, Apr 40k. the bars grow up from zero one after another, each labelled with its month and value"
    Then expect edit.width = 1920 and edit.height = 1080
    And expect edit.frames >= 115
    And the judge sees "a bar chart with four bars labelled Jan to Apr; short or missing bars early, four bars of increasing height with value labels by the end, the April bar the tallest" at frames 5, 50, 115
    And it costs under $0.20

  @level:easy
  Scenario: A ten-second countdown
    Given an empty space
    When the person says "make a 10-second countdown from 10 to 1, one big number centred on screen each second, with a quick pop as each number appears"
    Then expect edit.frames >= 295 and edit.frames <= 310
    And the judge sees "one large number centred on screen: 10 near the start, a middle number such as 5 or 6 halfway, 1 near the end" at frames 15, 165, 285
    And it costs under $0.10

  @level:medium
  Scenario: A lower-third pack
    Given an empty space
    When the person says "make a pack of three lower thirds for a 1920x1080 podcast: 'Maya Chen, Host', 'Dev Patel, Guest', 'Sam Ortiz, Producer'. each one slides in from the left, holds for 3 seconds, slides out, then the next one. same style for all three: a coloured bar with the name in bold and the role underneath"
    Then expect edit.width = 1920 and edit.height = 1080
    And expect edit.frames >= 270
    And the judge sees "a lower-third name tag in the bottom part of a 16:9 frame: Maya Chen (Host) first, Dev Patel (Guest) next, Sam Ortiz (Producer) last, all in the same style" at frames 45, 145, 245
    And it costs under $0.20
