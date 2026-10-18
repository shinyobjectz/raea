@medium:scene3d
Feature: Materials, lights and camera moves on the 3D hook
  The hook example: an orange cube on a floor, a ring of spheres, one sun light, a camera at z 9 looking at the cube.

  Background:
    Given the space "hook"

  @level:easy
  Scenario: A mirror floor
    When the person says "make the floor polished glossy black, like a mirror"
    Then expect P.rough <= 0.2 if plane P
    And expect lum(P.color) <= 0.08 if plane P
    And the judge sees "a 3D scene whose floor is glossy black, showing reflections or bright highlights" at frames 30, 120
    And it costs under $0.10

  @level:medium
  Scenario: Brushed gold
    When the person says "make the cube look like brushed gold metal"
    Then expect hue(B.color) >= 35 and hue(B.color) <= 55 if cube B
    And expect B.metal >= 0.8 if cube B
    And expect B.rough >= 0.2 and B.rough <= 0.6 if cube B
    And the judge sees "the central cube looks like gold metal, with soft brushed highlights rather than a mirror finish" at frames 30, 120
    And it costs under $0.25

  @level:easy
  Scenario: A blue rim light
    When the person says "add a second light: a cool blue rim light from behind the cube, keep the sun as it is"
    Then expect count(light) >= 2
    And expect L.lux >= 8000 if light L {id: sun}
    And the judge sees "the cube lit from the front as before, with a cool blue edge light along its outline from behind" at frames 30, 120
    And it costs under $0.10

  @level:medium
  Scenario: Orbit halfway round
    When the person says "orbit the camera halfway around the cube over the whole video, at the same height, always looking at the cube"
    Then expect C.z >= 8 if camera C, frame = 0
    And expect C.z <= -6 if camera C, frame = 149
    And expect C.y >= 1.8 and C.y <= 2.6 if camera C
    And the judge sees "the cube seen from the front first, from the side mid-way, from behind at the end, always centred in view" at frames 0, 75, 149
    And it costs under $0.25
