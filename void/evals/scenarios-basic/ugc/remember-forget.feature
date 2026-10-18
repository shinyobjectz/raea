@medium:ugc
Feature: Remembering and forgetting what the person decided
  Over a conversation the person sets rules, changes their mind and takes rules back. The agent has to keep a rule
  through later requests, and drop it once the person retires it.

  Background:
    Given the space "ugc"

  @level:medium
  Scenario: No yellow, then make the hook pop
    When the person says "rule for this client: no yellow anywhere, it's their competitor's colour"
    And the person says "now make the hook pop more"
    Then expect not (hue(T.bg) >= 40 and hue(T.bg) <= 70 and sat(T.bg) >= 0.35) if title T
    And expect not (hue(T.color) >= 40 and hue(T.color) <= 70 and sat(T.color) >= 0.35) if title T
    And expect not (hue(C.color) >= 40 and hue(C.color) <= 70 and sat(C.color) >= 0.35) if caption C
    And the judge sees "a hook title that stands out at the top, and no yellow anywhere on screen" at frames 20, 60
    And it costs under $0.25

  @level:hard
  Scenario: A size rule, a request against it, then the rule retired
    When the person says "from now on the captions never go below 70px, people watch these on small phones"
    And the person says "make the captions 64px, they're crowding her face"
    And the person says "fine, 64 is ok now. drop the 70 rule and do it"
    Then expect C.size >= 62 and C.size <= 66 if caption C
    And it costs under $0.50

  @level:medium
  Scenario: Captions to the top, then back but bigger
    When the person says "move the captions up to the top of the frame"
    And the person says "actually, put them back where they were, but bigger"
    Then expect C.y >= 1240 and C.y <= 1420 if caption C
    And expect C.size >= 82 if caption C
    And the judge sees "large captions in the lower part of the frame, below the face, readable" at frames 60, 400
    And it costs under $0.25

  @level:hard
  Scenario: B-roll removed, then back for the last second only
    When the person says "drop the b-roll at the end, I want her on camera for the payoff"
    And the person says "actually bring that b-roll back, but only for the last second"
    Then expect count(clip) >= 8
    And expect C.from >= 700 and C.from <= 715 and C.to >= 735 if clip C {z: 1}, C.from > 400
    And the judge sees "the speaker on camera during the payoff, then cutaway footage only in the final second" at frames 640, 725
    And it costs under $0.50
