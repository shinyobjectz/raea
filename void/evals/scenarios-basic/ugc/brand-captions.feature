@medium:ugc
Feature: Captions styled to a brand on the talking-head ad
  Brand kits arrive as a font and a colour; the captions and the hook take them on together.

  Background:
    Given the space "ugc"

  @level:medium
  Scenario: Captions on a dark pill
    When the person says "brand caption style: Montserrat, extra bold, white text on a solid black rounded box behind each caption"
    Then expect C.font = "Montserrat" if caption C
    And expect C.size >= 60 if caption C
    And the judge sees "white bold captions, each sitting on a solid black rounded box, fully readable over the video" at frames 60, 300, 500
    And it costs under $0.25

  @level:easy
  Scenario: A teal brand kit
    When the person says "our brand colour is teal #00b3a4. make the captions that teal, and make the hook pill the same teal with white text"
    Then expect hue(C.color) >= 165 and hue(C.color) <= 185 and sat(C.color) >= 0.5 if caption C
    And expect hue(T.bg) >= 165 and hue(T.bg) <= 185 if title T {id: hook}
    And expect lum(T.color) >= 0.9 if title T {id: hook}
    And the judge sees "teal captions and a teal hook pill with white text, all readable" at frames 30, 300
    And it costs under $0.10
