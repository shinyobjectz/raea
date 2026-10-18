Feature: The hook's brief
  What the hook must do, checked against the live edit. The edit itself is the SQL tables; this brief never changes
  them. The retired rrl checked edits against it (deleted 2026-10-17); the scenarios remain as the brief.

  Background:
    When the edit is playing

  Scenario: The hook lands in the first half second
    Given the title "hook"
    When it is past frame 14
    Then it is fully visible

  Scenario: The subtitle waits its turn
    Given the title "sub"
    When it is before frame 14
    Then it is hidden

  Scenario: Titles stay inside the frame
    Given the titles
    Then they stay inside the frame

  Scenario: The hook is on brand
    Given the title "hook"
    Then its colour is "#fff8ec"

  Scenario: Captions keep clear of the speaker's face
    Given the captions
    When a face is on screen
    Then they sit above the face by at least 24px

  Scenario: Captions are readable on a phone
    Given the captions
    Then their text is at least 56px

  Scenario: The hero keeps turning
    Given the cube "hero"
    Then it turns at least 1 degrees per frame

  Scenario: The ring holds its orbit
    Given the spheres in "ring"
    Then they stay within 2.5 of the centre
