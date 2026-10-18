@medium:ugc
Feature: Hooks, titles and end cards on the talking-head ad
  The hook title (yellow pill, first 2.5 s) and the end card (dark pill, last 1.5 s).

  Background:
    Given the space "ugc"

  @level:easy
  Scenario: A new hook line
    When the person says "change the hook to 'Stop automating your creatives'"
    Then expect T.text = "Stop automating your creatives" if title T, T.from < 30
    And the judge sees "a hook title near the top reading 'Stop automating your creatives'" at frames 20, 60
    And it costs under $0.10

  @level:medium
  Scenario: A hook that hits instantly
    When the person says "the hook is too soft. rewrite it as a question that makes people stop, and have it on screen fully from the very first frames, no slow fade"
    Then expect not T.text = "Automate ≠ simulate" if title T
    And expect T.opacity >= 0.99 if title T, T.from < 30, frame >= T.from + 3, frame < T.to
    And the judge sees "a hook title near the top that reads as a question, fully visible right away" at frames 3, 40
    And it costs under $0.20

  @level:medium
  Scenario: A follow CTA end card
    When the person says "swap the end card for a call to action that says 'Follow for part 2', make it look like a button, and keep it up for the last two seconds"
    Then expect false if frame = 0, not title _ {text: "Follow for part 2"}
    And expect T.to - T.from >= 55 and T.to >= 730 if title T {text: "Follow for part 2"}
    And expect not T.text = "Part 2: mapping the agency" if title T
    And the judge sees "a button-like call to action reading 'Follow for part 2' over the closing shot" at frames 700, 730
    And it costs under $0.20

  @level:easy
  Scenario: Brand colours on the titles
    When the person says "our brand colours are purple #6c3cff and white. restyle the hook and the end card to match, no more yellow"
    Then expect not (hue(T.bg) >= 40 and hue(T.bg) <= 70 and sat(T.bg) >= 0.35) if title T
    And expect hue(T.bg) >= 245 and hue(T.bg) <= 265 if title T {id: hook}
    And the judge sees "the hook title and end card use a purple and white look, no yellow" at frames 30, 720
    And it costs under $0.10
