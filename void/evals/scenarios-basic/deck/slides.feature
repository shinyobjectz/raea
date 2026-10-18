@medium:deck
Feature: Decks as timed slides
  A deck is 2D rows on a timeline, each slide a span of frames. Exporting one image per slide needs pages.

  @level:easy
  Scenario: A four-slide launch deck
    Given an empty space
    When the person says "make a 4-slide product launch deck at 1920x1080 for a note-taking app called Inkwell: title, the problem, how it works, pricing. 3 seconds per slide"
    Then expect edit.width = 1920 and edit.height = 1080
    And expect edit.frames >= 355 and edit.frames <= 365
    And the judge sees "four distinct, readable slides for 'Inkwell' in order: a title slide, a problem slide, a how-it-works slide, a pricing slide" at frames 45, 135, 225, 315
    And it costs under $0.10

  @level:medium
  Scenario: A themed talk deck
    Given an empty space
    When the person says "make a 3-slide talk deck at 1920x1080 called 'Shipping weekly': a dark navy background, white headlines, one orange accent on every slide. slides: the title, 'why weekly', 'what it takes'. 2 seconds each"
    Then expect edit.width = 1920 and edit.height = 1080
    And expect hue(C.bg) >= 200 and hue(C.bg) <= 250 and lum(C.bg) <= 0.25 if camera C
    And the judge sees "three slides on a dark navy background with white headlines and an orange accent: 'Shipping weekly', then 'why weekly', then 'what it takes'" at frames 30, 90, 150
    And it costs under $0.20

  @level:medium @needs:pages
  Scenario: A quarterly review exported as pages
    Given an empty space
    When the person says "make a 6-slide quarterly review deck at 1920x1080 (title, highlights, revenue, customers, risks, next quarter), 2 seconds per slide, and export each slide as its own image"
    Then expect edit.width = 1920 and edit.height = 1080
    And expect edit.frames >= 355
    And the judge sees "six distinct, readable slides in order: title, highlights, revenue, customers, risks, next quarter" at frames 30, 90, 150, 210, 270, 330
    And it exports 6 pages
    And it costs under $0.20
