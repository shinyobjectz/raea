# Prompt rewritten 2026-10-16 to state goals, not solutions: it named the parts (punch-ins, word captions, a hook
# title). The `T.y >= 220 if title T` check graded a part the prompt no longer asks for; it's replaced by the length
# asked for, and the judge line names outcomes.
@medium:ugc @level:hard
Feature: An explainer cut from a raw take
  The user's own test (2026-10-15): CJ's raw 75 s Instagram take, turned into a talking-head explainer.

  Scenario: CJ's raw take into a captioned explainer
    Given an empty space
    When the person says "~/.raea-void/media/cj-ig-post-2.mov is my raw take for an Instagram Reel. Make it into a 30 to 45 second explainer that people watch to the end."
    Then expect edit.frames >= 900 and edit.frames <= 1350
    And the judge sees "a vertical explainer that holds attention from the first second; anything on screen is readable, inside the platform's safe zones, and never covers the speaker's face" at frames 15, 300, 900
    And it costs under $1.00
