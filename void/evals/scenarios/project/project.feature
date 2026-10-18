# Project management (data/project.feature; the founder, 2026-10-17): a request is a task, a question is answered by
# choosing, every render is a version, an object goes back to an earlier version, and a project holds several objects
# the person switches between. Graded on the project's record (raea status, objects, versions), not on craft: the
# craft scenarios grade the pieces.
@medium:project
Feature: Tasks, questions, versions and objects across turns

  @level:medium
  Scenario: Pong, a change, and back to the first version
    Given an empty space
    When the person says "create a game of pong with sound effects"
    And the person says "make the ball faster"
    And the person says "actually go back to the version from before the faster ball, I want to play that one"
    Then the record shows "(game"
    And the object "pong" has 2 versions
    And the object "pong" changed in turn 2
    And the object "pong" is back where turn 1 left it
    And it costs under $3.00

  @level:medium
  Scenario: A UGC ad that needs the person's call
    Given the space "real"
    When the person says "media/takes has takes of a client's head of digital technology pitching one idea. Make a 20 second vertical post from them. I can't decide whether it should open on the joke or on the claim, so ask me which before you build it."
    And the person answers "#1"
    Then a question was asked
    And the record shows "t1 ["
    And the object "eval" has 1 versions
    And it costs under $3.00

  @level:hard
  Scenario: An ad and a game in one project, switching between them
    Given the space "real"
    When the person says "media/takes has takes of a client's head of digital technology pitching one idea. Make a 15 second vertical post from them."
    And the person says "now make a small pong game, separate from the ad"
    And the person says "back to the ad: make its opening text bigger"
    Then the record shows "(game"
    And the record shows "(video"
    And the object "pong" has 1 versions
    And the object "eval" changed in turn 3
    And it costs under $4.00
