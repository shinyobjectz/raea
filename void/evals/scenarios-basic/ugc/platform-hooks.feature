@medium:ugc
Feature: Hooks cut for different platforms on the talking-head ad
  The same ad, its opening reworked for where it will run: a calmer hook for LinkedIn, a second hook line for TikTok,
  a short teaser for YouTube Shorts.

  Background:
    Given the space "ugc"

  @level:medium
  Scenario: A calmer hook for LinkedIn
    When the person says "this one's going on LinkedIn. rewrite the hook so it reads calm and professional: sentence case, no symbols like the ≠, and keep it on screen for the first three seconds"
    Then expect not T.text = "Automate ≠ simulate" if title T, T.from < 30
    And expect T.to >= 85 and T.to <= 100 if title T, T.from < 30
    And the judge sees "a calm, professional hook title in sentence case near the top, with no symbols such as ≠, still on screen at the later frame" at frames 15, 80
    And it costs under $0.25

  @level:easy
  Scenario: A second hook line for TikTok
    When the person says "for TikTok, add a smaller second line under the hook that says exactly 'here's the difference', on screen at the same time as the hook"
    Then expect false if frame = 0, not title _ {text: "here's the difference"}
    And expect T.y >= H.y + 90 and T.size < H.size and T.from < 30 if title T {text: "here's the difference"}, title H {id: hook}
    And the judge sees "a hook title near the top with a smaller second line 'here's the difference' directly under it, the two not overlapping" at frames 20, 60
    And it costs under $0.10

  @level:hard
  Scenario: A teaser for YouTube Shorts
    When the person says "cut a teaser for YouTube Shorts: keep only the first three sentences, up to 'and the difference matters', then go straight to the end card. drop everything in between, close the gap, and keep the captions in sync"
    Then expect edit.frames >= 200 and edit.frames <= 300
    And expect false if frame = 0, not caption _ {text: "And the difference matters"}
    And expect not C.text = "it runs" if caption C
    And expect C.to <= edit.frames if caption C
    And expect false if frame = 0, not title _ {text: "Part 2: mapping the agency"}
    And expect T.to >= edit.frames - 1 if title T {text: "Part 2: mapping the agency"}
    And the judge sees "a short vertical talking-head teaser: the hook title at the start, captions mid-way, and the 'Part 2' end card at the close, with no black or empty frames" at frames 15, 100, 195
    And it costs under $0.50
