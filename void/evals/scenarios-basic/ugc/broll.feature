@medium:ugc
Feature: B-roll changes on the talking-head ad
  Two b-roll clips sit over the talking head at z 1: a screen recording over the explanation, a second clip over
  the payoff.

  Background:
    Given the space "ugc"

  @level:easy
  Scenario: Drop the second b-roll
    When the person says "get rid of the b-roll at the end, I want her on camera for the payoff"
    Then expect C.to <= 593 if clip C {z: 1}
    And the judge sees "the speaker's face fills the frame during the closing lines, no cutaway footage" at frames 620, 680
    And it costs under $0.10

  @level:easy
  Scenario: Cover the whole sentence
    When the person says "extend the screen recording so it covers the whole 'automating means handing someone's job to a model' sentence, start to finish"
    Then expect B.from <= 189 and B.to >= 342 if clip B {z: 1}, B.from < 400
    And it costs under $0.10
