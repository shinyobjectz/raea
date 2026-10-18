@medium:ugc
Feature: Sound design on the talking-head ad
  Foley comes from sao (free); a narrator line from fish (paid). Each new sound is an audio row next to the music
  bed, timed to the picture it belongs to.

  Background:
    Given the space "ugc"

  @level:easy
  Scenario: A whoosh on the cut
    When the person says "add a whoosh on the cut into the screen-recording b-roll"
    Then expect count(audio) >= 2
    And expect A.from >= 200 and A.from <= 222 if audio A, not A.id = "music"
    And it costs under $0.10

  @level:medium
  Scenario: Pops on the titles
    When the person says "add a soft pop sound as the hook appears and another one as the end card appears"
    Then expect count(audio) >= 3
    And expect A.from <= 10 or (A.from >= 680 and A.from <= 700) if audio A, not A.id = "music"
    And it costs under $0.25

  @level:hard
  Scenario: A riser into the end card
    When the person says "build tension into the end card: a riser that starts about two seconds before it and peaks as it lands, with a hit right on the end card. keep the mix inside the brief"
    Then expect count(audio) >= 2
    And expect A.from >= 615 and A.from <= 700 if audio A, not A.id = "music"
    And expect M.lufs = -14 if master M
    And it costs under $0.50

  @level:medium
  Scenario: A narrator over the end card
    When the person says "add a narrator voice over the end card saying 'Part two is up next', a calm voice-over, not her"
    Then expect count(audio) >= 2
    And expect A.from >= 680 and A.from <= 715 if audio A, not A.id = "music"
    And it costs under $0.25
