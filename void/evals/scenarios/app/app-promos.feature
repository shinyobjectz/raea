# Prompts rewritten 2026-10-16 to state goals, not solutions: the client's facts (platform, length, audience, the
# product by name, what the viewer should do) and nothing about parts, structure or style. Implementation checks
# (count(clip)) removed; the judge lines name outcomes.
@medium:app
Feature: App promos from real screen recordings
  Real footage only. media/arock-app.mov (77 s, 2016x1734) records arock, an app where you make a pet rock: name it,
  pick what it's made of, moss or grass and how smooth, its colour, what it speaks. media/jetflow-app.mp4 (6 min,
  1920x1080) walks through jetflow, a workflow product.

  Background:
    Given the space "real"

  @level:hard @design
  Scenario: A 15-second arock promo
    When the person says "media/arock-app.mov is a screen recording of arock, our app where you make your own pet rock. We need a 15 second vertical promo for TikTok and Reels, for people who've never heard of arock. We want them to download it."
    Then expect edit.width = 1080 and edit.height = 1920
    And expect edit.frames >= 390 and edit.frames <= 480
    And the judge sees "someone who has never seen arock understands what the app does and that they can download it, and the real app is legible on a phone screen"
    And it costs under $1.50

  @level:hard @design
  Scenario: A 30-second jetflow explainer
    When the person says "media/jetflow-app.mp4 is a 6 minute walkthrough of jetflow, our workflow product. We need a 30 second vertical video for TikTok and Reels that explains jetflow to people who've never seen it and gets them to try it."
    Then expect edit.width = 1080 and edit.height = 1920
    And expect edit.frames >= 840 and edit.frames <= 1050
    And the judge sees "someone who has never seen jetflow understands what it does, and the product is legible on a phone screen"
    And it costs under $2.00
