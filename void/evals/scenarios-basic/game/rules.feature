@medium:game
Feature: Game scenes played by rules
  Game scenes whose motion comes from rules over time, with no player input. Nothing draws rect or
  circle, so shapes are boxes.

  @level:hard
  Scenario: A breakout scene
    Given an empty space
    When the person says "make a 5-second breakout game scene, vertical: rows of coloured bricks at the top, a paddle near the bottom that slides to meet the ball, a ball bouncing between them, and the bricks it hits disappear"
    Then expect B.x >= -60 and B.x <= 1140 and B.y >= -60 and B.y <= 1980 if box B
    And the judge sees "a breakout game: rows of bricks near the top, a paddle near the bottom, a ball in a different place each frame, fewer bricks in later frames" at frames 0, 60, 140
    And it costs under $0.40

  @level:medium
  Scenario: A flappy-style scene
    Given an empty space
    When the person says "make a 6-second flappy-bird style scene: a small bird that bobs up and down on its own, green pipes with gaps scrolling in from the right, a score counter at the top"
    Then expect edit.frames >= 175
    And the judge sees "a side-scrolling scene with a small bird, green pipes with gaps in different positions between frames, and a score at the top" at frames 10, 90, 170
    And it costs under $0.20
