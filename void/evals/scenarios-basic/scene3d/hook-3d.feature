@medium:scene3d
Feature: Changes to a 3D product hook
  The hook example: a turning cube, a ring of spheres, titles, a caption that dodges a face.

  Background:
    Given the space "hook"

  @level:easy
  Scenario: A blue cube
    When the person says "make the cube blue"
    Then expect hue(B.color) >= 190 and hue(B.color) <= 250 if cube B
    And the judge sees "the central cube is blue" at frames 30, 120
    And it costs under $0.10

  @level:medium
  Scenario: Spin twice as fast
    When the person says "make the cube spin twice as fast"
    Then expect B.rot_y >= 6.0 if cube B, frame = 90
    And it costs under $0.15

  @level:hard
  Scenario: A glowing ring of light
    When the person says "replace the ring of spheres with a ring of small glowing cubes that pulse in size"
    Then expect not S.class = "ring" if sphere S
    And the judge sees "a ring of small cubes around the central cube, no spheres, sizes varying between frames" at frames 20, 60, 100
    And it costs under $0.30
