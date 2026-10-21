@medium:deck
Feature: More decks as timed slides
  A 1920x1080 edit, one `slide` row per page with the page's text shown over its span.

  @level:easy
  Scenario: A bakery deck
    Given an empty space
    When the person says "make a 3-slide deck at 1920x1080 for a bakery called Crumb & Co: a title slide, a menu slide with three items and prices, a 'visit us' slide with the address 12 Mill Lane. 2 seconds per slide"
    Then expect edit.width = 1920 and edit.height = 1080
    And expect edit.frames >= 175 and edit.frames <= 185
    And expect count(slide) = 3
    And the judge sees "three readable slides for 'Crumb & Co' in order: a title slide, a menu with three items and prices, a 'visit us' slide with 12 Mill Lane" at frames 30, 90, 150
    And it costs under $0.10

  @level:medium
  Scenario: Slide numbers in the corner
    Given an empty space
    When the person says "make a 5-slide workshop agenda deck at 1920x1080, 2 seconds per slide: a title slide 'Hooks that hold', then one slide each for 'Warm-up', 'Teardown', 'Write three hooks', 'Share-out'. put the slide number, 1 to 5, in the bottom-right corner of every slide"
    Then expect edit.width = 1920 and edit.height = 1080
    And expect edit.frames >= 295 and edit.frames <= 305
    And expect count(slide) = 5
    And the judge sees "five slides in order ('Hooks that hold', 'Warm-up', 'Teardown', 'Write three hooks', 'Share-out'), each with its own page number 1 to 5 in the bottom-right corner" at frames 30, 90, 150, 210, 270
    And it costs under $0.25

  @level:medium
  Scenario: A footer on every slide
    Given an empty space
    When the person says "make a 4-slide case-study deck at 1920x1080, 2.5 seconds per slide: client, challenge, what we did, results. every slide has the same thin footer bar across the bottom with the agency name 'Northstar' in it"
    Then expect edit.width = 1920 and edit.height = 1080
    And expect edit.frames >= 295 and edit.frames <= 305
    And expect count(slide) = 4
    And expect count(box) >= 1
    And the judge sees "four slides (client, challenge, what we did, results), each with the same thin footer bar along the bottom reading 'Northstar'" at frames 37, 112, 187, 262
    And it costs under $0.25
