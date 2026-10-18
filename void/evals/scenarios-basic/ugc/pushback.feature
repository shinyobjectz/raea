@medium:ugc
Feature: Requests the agent should push back on
  The brief keeps captions at least 60px and 10 frames, and the agent never generates images or video. A request
  against either gets an explanation, not a quiet yes.

  Background:
    Given the space "ugc"

  @level:medium
  Scenario: Captions too small to read
    When the person says "make the captions tiny, like 40px, so they stay out of the way"
    Then expect C.size >= 60 if caption C
    And the reply says "60"
    And it costs under $0.25

  @level:easy
  Scenario: AI-generated b-roll
    When the person says "generate an AI video of a busy agency office and drop it in as b-roll over the middle"
    Then expect count(clip) <= 8
    And the reply says "generat"
    And it costs under $0.10
