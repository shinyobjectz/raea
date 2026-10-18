@medium:doc
Feature: One-page documents as stills
  A single page held on screen: judged on readability and structure (a clear title, sections, hierarchy).

  @level:easy
  Scenario: A recipe card
    Given an empty space
    When the person says "make a 3-second still of a recipe card at 1080x1350 for 'Miso butter noodles': the title, a short ingredients list (noodles, white miso, butter, garlic, scallions) and four numbered steps"
    Then expect edit.width = 1080 and edit.height = 1350
    And expect edit.frames >= 85 and edit.frames <= 95
    And the judge sees "a readable recipe card titled 'Miso butter noodles' with an ingredients list and four numbered steps, clearly separated sections" at frames 0, 45
    And it costs under $0.10

  @level:medium
  Scenario: A changelog
    Given an empty space
    When the person says "make a 3-second still of a changelog page at 1080x1350 for 'Version 2.3': three sections headed Added, Fixed and Changed, two short bullet lines under each, and the date 'Oct 2026' under the title"
    Then expect edit.width = 1080 and edit.height = 1350
    And expect edit.frames >= 85 and edit.frames <= 95
    And the judge sees "a changelog page titled 'Version 2.3' with 'Oct 2026' beneath, then sections Added, Fixed, Changed each with two bullet lines; clear headings, consistent spacing, nothing cut off" at frames 0, 45
    And it costs under $0.25

  @level:medium
  Scenario: A product one-pager
    Given an empty space
    When the person says "make a 3-second still of a portrait one-pager at 1240x1754 for a podcast mic called Halo: a headline, a one-sentence pitch, three feature blocks side by side, a short spec list, and the price $129 at the bottom"
    Then expect edit.width = 1240 and edit.height = 1754
    And expect edit.frames >= 85 and edit.frames <= 95
    And the judge sees "a structured one-pager for 'Halo': a headline, a pitch line, three feature blocks side by side, a spec list and the price $129 at the bottom; readable, aligned, nothing overlapping" at frames 0, 45
    And it costs under $0.25
