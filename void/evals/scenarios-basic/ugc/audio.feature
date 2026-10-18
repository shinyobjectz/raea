@medium:ugc
Feature: Music, ducking and loudness on the talking-head ad
  The bed sits at -4 dB and ducks to -18 under speech; the master is -14 LUFS with a -1.5 dB ceiling, and the brief
  holds the mix to -14 LUFS within 1 LU.

  Background:
    Given the space "ugc"

  @level:easy
  Scenario: Duck the music less
    When the person says "the music disappears when she talks, only duck it to -12 dB"
    Then expect A.gain >= -12.5 and A.gain <= -11.5 if audio A {id: music}, speech W, frame >= W.from + 12, frame < W.to - 6
    And it costs under $0.10

  @level:medium
  Scenario: No music under the hook
    When the person says "no music during the hook, bring the bed in once the hook title is gone"
    Then expect A.gain <= -40 if audio A, live(A), frame < 70
    And expect A.gain >= -20 if audio A {id: music}, live(A), frame >= 100, frame < 126
    And it costs under $0.20

  @level:easy
  Scenario: A safer ceiling
    When the person says "set the master ceiling to -2 dB, some platforms clip it"
    Then expect M.ceiling <= -2 and M.ceiling >= -2.5 if master M
    And expect M.lufs = -14 if master M
    And it costs under $0.10

  @level:hard
  Scenario: Louder than the brief allows
    When the person says "master it to -10 LUFS, I want it louder than everything else in the feed"
    Then expect M.lufs >= -15 and M.lufs <= -13 if master M
    And the reply says "-14"
    And it costs under $0.40
