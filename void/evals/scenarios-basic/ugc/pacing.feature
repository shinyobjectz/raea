@medium:ugc
Feature: Pacing on the talking-head ad
  Tightening, cutting and reordering CJ's sentences. The captions, b-roll and speech rows have to follow the clips
  they belong to, or the ad drifts out of sync.

  Background:
    Given the space "ugc"

  @level:medium
  Scenario: Under 22 seconds
    When the person says "this drags a bit, tighten the pauses between sentences so the whole ad is under 22 seconds. don't cut any lines"
    Then expect edit.frames <= 660
    And expect C.to <= edit.frames if caption C
    And the judge sees "a vertical talking-head video with readable captions that match a continuous delivery, no black or empty frames" at frames 30, 300, 600
    And it costs under $0.20

  @level:medium
  Scenario: Cut a sentence
    When the person says "cut the 'and the difference matters' line completely and close the gap"
    Then expect not C.text = "And the difference matters" if caption C
    And expect edit.frames <= 710
    And expect C.to <= edit.frames if caption C
    And it costs under $0.20

  @level:hard
  Scenario: Open on the strongest line
    When the person says "open with the 'automating means handing someone's job to a model and seeing what breaks' sentence, then go into 'all right, so we're not automating...' and carry on from there. keep every line, keep the captions in sync"
    Then expect W.from < V.from if caption W {text: "job to a model"}, caption V {text: "All right"}
    And expect edit.frames >= 700
    And the judge sees "a talking-head video whose captions read as complete sentences in a sensible order, with no gaps or black frames" at frames 15, 200, 450
    And it costs under $0.40
