@medium:ugc
Feature: Quick edits to a talking-head ad
  The UGC example (CJ on automating vs simulating): one clip per sentence, captions in one style, a hook title, a
  ducked music bed. Each scenario is one request a creator makes mid-edit.

  Background:
    Given the space "ugc"

  @level:easy
  Scenario: Yellow captions
    When the person says "make the captions yellow"
    Then expect hue(C.color) >= 40 and hue(C.color) <= 70 and sat(C.color) >= 0.35 if caption C
    And the judge sees "the on-screen captions are yellow and easy to read" at frames 30, 300
    And it costs under $0.10

  @level:easy
  Scenario: A bigger hook title
    When the person says "make the hook title bigger"
    Then expect T.size >= 72 if title T {id: hook}
    And it costs under $0.10

  @level:easy
  Scenario: Drop the end card
    When the person says "remove the end card, we don't need part 2"
    Then expect not T.text = "Part 2: mapping the agency" if title T
    And it costs under $0.10

  @level:medium
  Scenario: Captions in the upper third
    When the person says "move all the captions up into the top third of the frame, but keep them out of the top safe zone"
    Then expect C.y <= 700 if caption C
    And expect C.y >= 220 if caption C
    And the judge sees "captions sit in the upper third of the frame, below the very top edge, readable over the video" at frames 60, 400
    And it costs under $0.15

  @level:medium
  Scenario: Duck the music harder under speech
    When the person says "the music is too loud under her voice, duck it to -24 dB while she's talking"
    Then expect A.gain <= -23.5 if audio A {id: music}, speech W, frame >= W.from + 12, frame < W.to - 6
    And it costs under $0.15

  @level:hard
  Scenario: A lower third that stays in the safe zone
    When the person says "add a lower third that says 'CJ · creative strategist' for the first three seconds, styled like the hook, kept out of the platform buttons"
    Then expect T.y >= 220 and T.y <= 1460 if title T, T.from < 90
    And the judge sees "a lower-third name tag reading 'CJ · creative strategist' appears early, styled cleanly, not covering the face or the captions" at frames 15, 60
    And it costs under $0.25
