@medium:motion
Feature: Motion graphics from nothing

  @level:medium
  Scenario: A kinetic-type intro
    Given an empty space
    When the person says "make a 4-second kinetic typography intro that says 'Stop scrolling. Start building.' revealing word by word with eased motion"
    Then expect edit.frames >= 110
    And the judge sees "bold animated words reading 'Stop scrolling. Start building.' appearing over time; fewer words visible early, all of them by the end" at frames 10, 50, 110
    And it costs under $0.30
