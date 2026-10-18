@medium:ugc
Feature: Calls to action on the talking-head ad

  Background:
    Given the space "ugc"

  @level:easy
  Scenario: Link in bio
    When the person says "add a small 'Link in bio' tag for the last three seconds, in the lower middle of the frame just above the captions"
    Then expect false if frame = 0, not title _ {text: "Link in bio"}
    And expect T.from >= 640 and T.from <= 660 and T.to >= 735 if title T {text: "Link in bio"}
    And expect T.y >= 1000 and T.y + 40 <= 1340 if title T {text: "Link in bio"}
    And the judge sees "a small 'Link in bio' tag in the lower middle of the frame, above where the captions sit, not covering the face" at frames 660, 720
    And it costs under $0.10
