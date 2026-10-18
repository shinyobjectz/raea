@medium:deck
Feature: Decks handed over as pages
  Each slide exported as its own still (export_pages), so these wait for pages.

  @level:easy @needs:pages
  Scenario: An event deck as images
    Given an empty space
    When the person says "make a 3-slide deck at 1920x1080 for a meetup called 'Cut Night': the title with the date 'Nov 14', the schedule, how to sign up. 2 seconds per slide, then export each slide as an image"
    Then expect edit.width = 1920 and edit.height = 1080
    And expect count(slide) = 3
    And the judge sees "three readable slides in order: 'Cut Night' with 'Nov 14', a schedule, how to sign up" at frames 30, 90, 150
    And it exports 3 pages
    And it costs under $0.10

  @level:medium @needs:pages
  Scenario: A roadmap exported as pages
    Given an empty space
    When the person says "make a 4-slide product roadmap at 1920x1080, one slide per quarter (Q1 to Q4), each with a heading and three bullet points, 2 seconds per slide, and export it as one image per slide"
    Then expect edit.width = 1920 and edit.height = 1080
    And expect count(slide) = 4
    And the judge sees "four roadmap slides headed Q1, Q2, Q3, Q4 in order, each with three readable bullet points" at frames 30, 90, 150, 210
    And it exports 4 pages
    And it costs under $0.25

  @level:hard @needs:pages
  Scenario: A sales deck with a chart and a pricing table
    Given an empty space
    When the person says "make an 8-slide sales deck at 1920x1080 for a scheduling tool called Slotty, 2 seconds per slide: title, problem, solution, how it works, a bar chart of hours saved per week (2, 5, 9), a pricing table with three plans, a customer quote, and a contact slide. export every slide as an image"
    Then expect edit.width = 1920 and edit.height = 1080
    And expect count(slide) = 8
    And expect count(box) >= 3
    And the judge sees "eight readable slides for 'Slotty' in order, including a bar chart with three bars of increasing height and a pricing table with three plans" at frames 30, 90, 150, 210, 270, 330, 390, 450
    And it exports 8 pages
    And it costs under $0.50
