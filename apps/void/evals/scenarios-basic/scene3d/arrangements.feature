@medium:scene3d
Feature: Small 3D arrangements from an empty space
  Cubes, spheres and a floor under a light, built from nothing.

  @level:medium
  Scenario: Three cubes and a ball
    Given an empty space
    When the person says "make a 4-second 3D still life: three cubes in a row on a floor, a sphere resting on top of the middle cube, soft daylight, a fixed camera"
    Then expect edit.frames >= 115 and edit.frames <= 125
    And expect count(cube) = 3
    And expect count(sphere) = 1
    And expect S.y > max(cube.y) if sphere S
    And the judge sees "three cubes in a row on a floor with a sphere sitting on the middle one, softly lit" at frames 0, 60, 115
    And it costs under $0.25

  @level:hard
  Scenario: A tower that builds itself
    Given an empty space
    When the person says "make a 5-second 3D animation: five cubes drop in one at a time from above onto a floor, each landing on top of the last, ending as a neat tower"
    Then expect edit.frames >= 145 and edit.frames <= 155
    And expect count(cube) = 5
    And expect max(cube.y) - min(cube.y) >= 1.5 if frame = 149
    And the judge sees "few or no cubes stacked at first, a partial stack mid-way, a neat tower of five cubes at the end" at frames 5, 75, 149
    And it costs under $0.50
