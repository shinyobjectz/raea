@medium:ugc
Feature: Caption styling and timing on the talking-head ad
  The captions share one style, `cap`, and run three or four words at a time across each sentence.

  Background:
    Given the space "ugc"

  @level:easy
  Scenario: Impact captions
    When the person says "switch the caption font to Impact, it reads better on a phone"
    Then expect C.font = "Impact" if caption C
    And the judge sees "bold condensed Impact-style captions, still readable over the video" at frames 30, 400
    And it costs under $0.10

  @level:medium
  Scenario: Make the key line pop
    When the person says "make the caption that says 'We're simulating them' pop: yellow and noticeably bigger than the others"
    Then expect false if frame = 0, not caption _ {text: "We're simulating them"}
    And expect C.size >= 88 if caption C {text: "We're simulating them"}
    And the judge sees "the caption 'We're simulating them' is yellow and larger than the white captions around it" at frames 100, 140, 170
    And it costs under $0.20

  @level:hard
  Scenario: One word at a time
    When the person says "change the captions to one word at a time, each word timed to when she says it, punchy TikTok style"
    Then expect C.to - C.from <= 24 if caption C
    And expect C.to - C.from >= 4 if caption C
    And the judge sees "single-word captions, one word on screen at a time, large and readable, changing between frames" at frames 30, 40, 300, 310
    And it costs under $0.40

  @level:easy
  Scenario: Clear of the side buttons
    When the person says "the captions run under the like and share buttons on the right side. keep them inside the left 900 pixels of the frame"
    Then expect C.x + C.w <= 900 if caption C
    And expect C.x >= 30 if caption C
    And the judge sees "captions sit left of the right-hand edge where platform buttons would be, fully readable" at frames 60, 350
    And it costs under $0.10
