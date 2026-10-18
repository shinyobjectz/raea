@medium:motion
Feature: Explainer graphics from an empty space
  Cards, lists, bars and diagrams built from 2D rows (title, text, box) with keys and rules. Shapes and lines are
  boxes; nothing else draws.

  @level:easy
  Scenario: A chapter card
    Given an empty space
    When the person says "make a 3-second chapter card at 1920x1080 that says 'Chapter 2: The Map'. it fades in, holds, and fades out. white text on a near-black background"
    Then expect edit.width = 1920 and edit.height = 1080
    And expect edit.frames >= 85 and edit.frames <= 95
    And the judge sees "a near-black 16:9 frame: the text faint or absent at first, 'Chapter 2: The Map' clearly readable in the middle frame, fading at the end" at frames 1, 45, 88
    And it costs under $0.10

  @level:medium
  Scenario: A list that builds in
    Given an empty space
    When the person says "make a 5-second vertical list graphic on a cream background: the heading 'Three rules for hooks', then three numbered lines that build in one per second: 'Say it in 3 words', 'Show the result first', 'Cut every pause'"
    Then expect lum(C.bg) >= 0.8 if camera C
    And expect edit.frames >= 145 and edit.frames <= 155
    And the judge sees "a cream background with the heading 'Three rules for hooks'; one numbered line early, more later, all three lines by the end, none overlapping" at frames 15, 75, 145
    And it costs under $0.25

  @level:easy
  Scenario: A loading bar
    Given an empty space
    When the person says "make a 5-second loading animation at 1080x1080: a rounded bar in the middle fills from empty to full, with a percentage above it counting from 0% to 100%"
    Then expect edit.width = 1080 and edit.height = 1080
    And expect count(box) >= 1
    And expect min(box.w) <= 60 if frame = 1
    And expect max(box.w) >= 500 if frame = 148
    And the judge sees "a rounded bar in the middle of a square frame: nearly empty with a low percentage first, about half full mid-way, full with 100% at the end" at frames 3, 75, 148
    And it costs under $0.10

  @level:hard
  Scenario: A flow diagram that draws in
    Given an empty space
    When the person says "make a 5-second flow diagram at 1920x1080: four labelled boxes in a row, 'Brief', 'Script', 'Shoot', 'Edit', joined by lines that draw in from left to right, one connection per second"
    Then expect edit.width = 1920 and edit.height = 1080
    And expect edit.frames >= 145 and edit.frames <= 155
    And expect count(box) >= 7
    And the judge sees "four labelled boxes Brief, Script, Shoot, Edit in a left-to-right row; few or no connecting lines early, more mid-way, all three connections by the end" at frames 10, 75, 145
    And it costs under $0.50
