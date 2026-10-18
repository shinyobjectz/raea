@medium:game
Feature: Game screens rrl can play today
  Attract loops, scoreboards and game-over screens: motion from rules over time, no input. Shapes are boxes.

  @level:easy
  Scenario: A bouncing-ball attract loop
    Given an empty space
    When the person says "make a 4-second attract-mode loop at 1080x1080: a ball bouncing around inside a rectangular frame, never leaving it"
    Then expect edit.width = 1080 and edit.height = 1080
    And expect edit.frames >= 115 and edit.frames <= 125
    And expect count(box) >= 2
    And expect B.x >= 0 and B.y >= 0 and B.x + B.w <= 1080 and B.y + B.h <= 1080 if box B
    And the judge sees "a rectangular frame with a ball inside it, the ball in a different place each frame and always inside the frame" at frames 0, 40, 80, 115
    And it costs under $0.10

  @level:medium
  Scenario: An arcade scoreboard
    Given an empty space
    When the person says "make a 5-second arcade scoreboard at 1920x1080: 'P1' and its score on the left, 'P2' and its score on the right, 'HI 9999' centred at the top, and P1's score goes up by 10 every half second starting from 450"
    Then expect edit.width = 1920 and edit.height = 1080
    And expect edit.frames >= 145 and edit.frames <= 155
    And the judge sees "an arcade scoreboard: P1 with a score on the left, P2 on the right, HI 9999 at the top centre; P1's score is higher in each later frame" at frames 5, 75, 145
    And it costs under $0.25

  @level:easy
  Scenario: A flashing game-over screen
    Given an empty space
    When the person says "make a 3-second retro game-over screen: 'GAME OVER' flashing on and off twice a second, with 'PRESS START' and 'SCORE 1280' steady under it"
    Then expect edit.frames >= 85 and edit.frames <= 95
    And the judge sees "a retro game-over screen: 'GAME OVER' visible in some frames and hidden in others, 'PRESS START' and 'SCORE 1280' visible in every frame" at frames 3, 11, 18, 26
    And it costs under $0.10
