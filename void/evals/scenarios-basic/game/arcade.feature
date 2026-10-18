@medium:game
Feature: Games, as scenes today
  rrl has no input rows or game state yet (void/evals/gaps), so these ask for a game's scene and its motion, played
  by rules over time.

  @level:hard
  Scenario: A pong scene
    Given an empty space
    When the person says "make a 2D pong game scene at 1920x1080: two paddles at the sides, a ball bouncing between them, a score at the top"
    Then expect edit.width = 1920
    And the judge sees "a pong-like scene: one paddle on each side, a ball whose position changes between frames, a score at the top" at frames 0, 45, 90
    And it costs under $0.40

  @level:hard
  Scenario: A 3D runner scene
    Given an empty space
    When the person says "make a 3D endless runner scene: a player cube on a long track, obstacles moving toward the camera"
    Then the judge sees "a 3D scene with a track receding into the distance, a player object, obstacles closer to the camera in later frames" at frames 0, 60, 120
    And it costs under $0.40

  @level:hard @needs:play
  Scenario: Pong that plays
    Given an empty space
    When the person says "make a playable pong at 1920x1080: the left paddle follows the up and down keys, the right paddle tracks the ball, the ball bounces off paddles and walls, and the score goes up when it gets past a paddle. Add a scripted input track that holds 'up' from frame 30 to 60."
    Then expect P.y < 500 if box P {id: left}, frame = 60
    And the judge sees "a pong game: paddles on both sides, a ball in different places, a score at the top" at frames 0, 60, 120
    And it costs under $0.60

  @level:hard @needs:play
  Scenario: A runner that jumps
    Given an empty space
    When the person says "make a 3D runner where the player cube jumps when space is pressed and lands back on the track; add a scripted input track pressing space at frame 30"
    Then expect B.y > 1.0 if cube B {id: player}, frame = 40
    And expect B.y <= 0.6 if cube B {id: player}, frame = 120
    And it costs under $0.60
