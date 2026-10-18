@medium:game
Feature: Games that take input
  Input rows and scripted input tracks don't exist yet, so these wait for play.

  @level:medium @needs:play
  Scenario: A square that moves with the arrows
    Given an empty space
    When the person says "make a playable top-down game at 1080x1080: a square called player starts in the centre and moves with the arrow keys, stopping at the walls. add a scripted input track that holds 'right' from frame 0 to 60"
    Then expect P.x >= 700 if box P {id: player}, frame = 60
    And expect P.x + P.w <= 1080 if box P {id: player}
    And it costs under $0.25

  @level:hard @needs:play
  Scenario: Breakout with a paddle on the keys
    Given an empty space
    When the person says "make a playable breakout, vertical: a paddle called paddle starts centred near the bottom and follows the left and right keys, the ball bounces off the paddle, walls and bricks, and a brick it hits disappears. add a scripted input track that holds 'left' from frame 0 to 45"
    Then expect P.x <= 300 if box P {id: paddle}, frame = 45
    And the judge sees "a breakout game with bricks at the top, a paddle near the bottom left, and a ball in a different place each frame" at frames 0, 45, 120
    And it costs under $0.50

  @level:hard @needs:play
  Scenario: Click the target
    Given an empty space
    When the person says "make a playable click-the-target game at 1920x1080: a round target called target sits at a random spot, clicking it adds a point to a score row called score and moves the target. add a scripted input track that clicks the target's centre at frame 30"
    Then expect S.value >= 1 if score S, frame = 40
    And expect S.value = 0 if score S, frame = 10
    And it costs under $0.50
