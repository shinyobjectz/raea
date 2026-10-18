@medium:ugc
Feature: Revisions over a conversation on the talking-head ad
  The second request refines the first: the agent has to build on what it just did, not start over.

  Background:
    Given the space "ugc"

  @level:medium
  Scenario: Green, then a softer mint
    When the person says "make the captions green"
    And the person says "hmm, too loud. make it a softer mint green, and a touch smaller"
    Then expect hue(C.color) >= 120 and hue(C.color) <= 175 and lum(C.color) >= 0.6 if caption C
    And expect C.size < 76 and C.size >= 60 if caption C
    And the judge sees "pale mint-green captions, readable over the video" at frames 60, 400
    And it costs under $0.20

  @level:hard
  Scenario: A lower third, then moved and held longer
    When the person says "add a lower third that says 'CJ · creative strategist' at the start"
    And the person says "good, now move it down so it sits just above the captions, and keep it up for four seconds"
    Then expect false if frame = 0, not title _ {text: "CJ · creative strategist"}
    And expect T.to - T.from >= 115 and T.y >= 1000 and T.y <= 1320 if title T {text: "CJ · creative strategist"}
    And the judge sees "a lower-third name tag reading 'CJ · creative strategist' sits just above the captions, not overlapping them" at frames 30, 100
    And it costs under $0.40

  @level:easy
  Scenario: Red hook, then back to yellow but bigger
    When the person says "make the hook title red"
    And the person says "no, go back to the yellow, but make the hook text bigger"
    Then expect hue(T.bg) >= 40 and hue(T.bg) <= 70 and sat(T.bg) >= 0.35 if title T {id: hook}
    And expect T.size >= 72 if title T {id: hook}
    And it costs under $0.10
