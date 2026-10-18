@medium:scene3d
Feature: Camera, lighting and turntables in 3D
  The hook example (a turning cube, a ring of spheres, titles) and a product turntable from nothing.

  @level:medium
  Scenario: A slow dolly in
    Given the space "hook"
    When the person says "add a slow dolly in on the cube over the whole video, starting where the camera is now and ending much closer"
    Then expect C.z >= 8.5 if camera C, frame = 0
    And expect C.z <= 6.5 if camera C, frame = 149
    And the judge sees "the cube appears noticeably larger in later frames as the camera moves closer" at frames 0, 75, 149
    And it costs under $0.20

  @level:easy
  Scenario: Moody sunset light
    Given the space "hook"
    When the person says "make the lighting moody, like a dim sunset"
    Then expect L.lux <= 5000 if light L
    And the judge sees "a dimmer, warmer, moodier 3D scene than a bright daylight render; the cube still visible" at frames 30, 120
    And it costs under $0.10

  @level:hard
  Scenario: A product turntable
    Given an empty space
    When the person says "make a 5-second product turntable: one orange cube on a floor, turning exactly one full rotation over the video, fixed camera, soft studio light"
    Then expect B.rot_y <= 0.1 if cube B, frame = 0
    And expect B.rot_y >= 6.0 and B.rot_y <= 6.6 if cube B, frame = 149
    And the judge sees "a lit orange cube on a floor, seen from a fixed camera, turned to a different angle in each frame" at frames 0, 50, 100
    And it costs under $0.40
