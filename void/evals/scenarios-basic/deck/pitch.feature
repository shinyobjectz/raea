@medium:deck
Feature: Pitch decks
  A deck is pages of 2D rows on a timeline: each slide a span of frames, exported as stills.

  @level:medium
  Scenario: A three-slide pitch deck
    Given an empty space
    When the person says "make a 3-slide pitch deck for a coffee subscription called Brewly at 1920x1080: a title slide, a problem slide, a solution slide, each slide 2 seconds"
    Then expect edit.width = 1920 and edit.height = 1080
    And the judge sees "three distinct presentation slides for 'Brewly': a title slide, then a problem slide, then a solution slide, each with readable headline text" at frames 15, 75, 135
    And it costs under $0.40

  @level:medium @needs:pages
  Scenario: A deck exported as pages
    Given an empty space
    When the person says "make a 5-slide investor deck for Brewly at 1920x1080 (title, problem, solution, traction, ask) and export it as one image per slide"
    Then expect edit.width = 1920
    And the judge sees "five distinct, readable investor slides in order: title, problem, solution, traction, ask" at frames 15, 75, 135, 195, 255
    And it exports 5 pages
    And it costs under $0.60
