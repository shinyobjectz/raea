@medium:ugc
Feature: Reframing the talking-head ad for other formats
  The ad is 9:16 (1080x1920). A reframe changes the edit's size and moves whatever falls outside.

  Background:
    Given the space "ugc"

  @level:hard
  Scenario: A 4:5 feed version
    When the person says "reframe this edit to 4:5 for the Instagram feed, 1080x1350. keep the hook, captions and end card inside the frame and off her face"
    Then expect edit.width = 1080 and edit.height = 1350
    And expect C.y >= 60 and C.y + 100 <= 1350 if caption C
    And expect T.y >= 0 and T.y + 120 <= 1350 if title T
    And the judge sees "a 4:5 talking-head frame with the hook title, captions and end card all fully inside the frame and not covering the face" at frames 30, 400, 720
    And it costs under $0.40

  @level:easy
  Scenario: A square version
    When the person says "make this square, 1080 by 1080, for LinkedIn. captions near the bottom but fully in frame"
    Then expect edit.width = 1080 and edit.height = 1080
    And expect C.y >= 600 and C.y + 100 <= 1080 if caption C
    And the judge sees "a square talking-head frame with captions near the bottom, fully inside the frame" at frames 60, 400
    And it costs under $0.10
