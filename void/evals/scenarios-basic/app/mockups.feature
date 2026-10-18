@medium:app
Feature: App UI mockups
  Screens built from boxes and text, judged on layout and legibility: aligned rows, readable labels, nothing
  overlapping or cut off.

  @level:easy
  Scenario: A phone settings screen
    Given an empty space
    When the person says "make a 3-second still of a phone settings screen, 1080x1920: a 'Settings' header, then rows for Wi-Fi, Bluetooth, Notifications and Dark mode, each with a toggle on the right. light grey background, white rows"
    Then expect edit.frames >= 85 and edit.frames <= 95
    And expect count(box) >= 4
    And the judge sees "a phone settings screen: a 'Settings' header and four aligned white rows (Wi-Fi, Bluetooth, Notifications, Dark mode) each with a toggle at the right, all text readable and nothing overlapping" at frames 0, 45, 85
    And it costs under $0.10

  @level:medium
  Scenario: A checkout card
    Given an empty space
    When the person says "make a 3-second still of a checkout card at 1080x1080: one product line 'Linen tote · $48', then subtotal, shipping $6 and total $54 lined up on the right, and a wide 'Pay now' button at the bottom"
    Then expect edit.width = 1080 and edit.height = 1080
    And expect edit.frames >= 85 and edit.frames <= 95
    And expect count(box) >= 2
    And the judge sees "a clean checkout card: 'Linen tote · $48', subtotal, shipping $6 and total $54 with amounts aligned on the right, and a wide 'Pay now' button at the bottom; all text readable, nothing overlapping" at frames 0, 45
    And it costs under $0.25

  @level:hard
  Scenario: A dashboard of KPI tiles
    Given an empty space
    When the person says "make a 3-second still of an analytics dashboard at 1920x1080: four KPI tiles in a 2x2 grid with even gaps (Views 1.2M, Watch time 38k h, Followers +4,210, CTR 6.8%), each tile with its label, its number and a small bar sparkline of five bars"
    Then expect edit.width = 1920 and edit.height = 1080
    And expect edit.frames >= 85 and edit.frames <= 95
    And expect count(box) >= 24
    And the judge sees "a dashboard with four tiles in an even 2x2 grid, each showing a label, a large number (Views 1.2M, Watch time 38k h, Followers +4,210, CTR 6.8%) and a small bar sparkline; aligned, readable, nothing overlapping" at frames 0, 45
    And it costs under $0.50
