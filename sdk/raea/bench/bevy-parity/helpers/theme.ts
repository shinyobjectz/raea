// Bevy v0.20.0 examples/helpers/theme.rs in raea's TSX: basic_example_theme, the feathers theme the examples' option
// menus use (2d_shapes, wireframe_2d, the 3d examples with checkboxes), each token its own colour
// (ThemeProps::new_non_contextual), and the option menus' checkbox and corner node (helpers/checkbox.rs, scene.rs).
import { createElement, type ReactNode } from "react";
import { Oklcha, Srgba } from "raea/color";
import { FeathersCheckbox, caption, palette, tokens, type ThemeProps, type ThemeToken } from "raea/feathers";
import { Node, px } from "raea/ui";

/** palettes::basic::GRAY */
const GRAY = new Srgba(0.5019608, 0.5019608, 0.5019608, 1);
const BLACK = new Srgba(0, 0, 0, 1);
const WHITE = new Srgba(1, 1, 1, 1);

/** ThemeProps::new_non_contextual: each token its own semantic colour, of the token's name, with no surface overrides */
function new_non_contextual(init: [ThemeToken, Srgba | Oklcha][]): ThemeProps {
  const t: ThemeProps = { semantic_base: {}, semantic_overrides: {}, token_assignments: {} };
  for (const [token, c] of init) {
    t.token_assignments[token] = token;
    t.semantic_base[token] = c instanceof Oklcha ? c : Oklcha.from(c);
  }
  return t;
}

/** helpers/theme.rs basic_example_theme(text_color) */
export function basic_example_theme(text: Srgba): ThemeProps {
  const k = tokens, p = palette;
  return new_non_contextual([
    [k.RADIO_TEXT, text], [k.RADIO_MARK, BLACK], [k.RADIO_MARK_HOVER, BLACK], [k.RADIO_MARK_PRESSED, BLACK],
    [k.RADIO_BG, WHITE], [k.RADIO_BG_HOVER, GRAY], [k.RADIO_BG_PRESSED, GRAY], [k.RADIO_BG_CHECKED, WHITE], [k.RADIO_BG_CHECKED_HOVER, BLACK], [k.RADIO_BG_CHECKED_PRESSED, BLACK],
    [k.RADIO_BORDER, BLACK], [k.RADIO_BORDER_HOVER, GRAY], [k.RADIO_BORDER_PRESSED, BLACK], [k.RADIO_BORDER_CHECKED, BLACK], [k.RADIO_BORDER_CHECKED_HOVER, BLACK], [k.RADIO_BORDER_CHECKED_PRESSED, BLACK],
    [k.TEXT_INPUT_TEXT, text], [k.TEXT_INPUT_BG, BLACK], [k.TEXT_INPUT_LABEL_BG, BLACK],
    [k.TEXT_INPUT_CURSOR, p.ACCENT.lighter(0.2)], [k.TEXT_INPUT_SELECTION, p.ACCENT], [k.TEXT_INPUT_SELECTION_UNFOCUSED, p.TRANSPARENT],
    [k.CHECKBOX_TEXT, text], [k.CHECKBOX_BG, BLACK], [k.CHECKBOX_MARK, WHITE], [k.CHECKBOX_BORDER, BLACK], [k.CHECKBOX_BORDER_HOVER, p.GRAY_2], [k.CHECKBOX_BORDER_CHECKED_HOVER, p.GRAY_0],
    [k.CHECKBOX_BORDER_PRESSED, p.GRAY_1], [k.CHECKBOX_BORDER_CHECKED_PRESSED, p.GRAY_1], [k.CHECKBOX_BORDER_CHECKED, BLACK], [k.CHECKBOX_BG_HOVER, p.GRAY_0], [k.CHECKBOX_BG_CHECKED, BLACK],
    [k.CHECKBOX_BG_CHECKED_HOVER, BLACK], [k.CHECKBOX_BG_CHECKED_PRESSED, p.GRAY_1], [k.CHECKBOX_BG_PRESSED, BLACK],
    [k.BUTTON_BG, p.TRANSPARENT], [k.BUTTON_BG_HOVER, p.TRANSPARENT], [k.BUTTON_BG_PRESSED, p.TRANSPARENT], [k.MENU_BG, p.GRAY_0], [k.MENU_BORDER, p.GRAY_0],
    [k.MENUITEM_BG_HOVER, p.TRANSPARENT], [k.MENUITEM_BG_PRESSED, p.TRANSPARENT], [k.MENUITEM_BG_FOCUSED, p.TRANSPARENT], [k.LISTROW_BG, p.TRANSPARENT], [k.LISTROW_BG_HOVER, p.BLACK], [k.LISTROW_BG_SELECTED, p.BLACK],
    [k.BUTTON_TEXT, text], [k.LISTROW_TEXT, text],
    // the helper sets the slider tokens twice; the second, below, replaces the first, as HashMap::insert does
    [k.SLIDER_BG, p.BLACK], [k.SLIDER_BG_HOVER, p.GRAY_0], [k.SLIDER_BG_PRESSED, p.GRAY_1], [k.SLIDER_BAR, p.ACCENT], [k.SLIDER_BAR_HOVER, p.ACCENT.lighter(0.05)], [k.SLIDER_BAR_PRESSED, p.ACCENT.lighter(0.1)],
    [k.SLIDER_TEXT, text], [k.TEXT_MAIN, text],
  ]);
}

/** helpers/checkbox.rs feathers_option_checkbox: a feathers checkbox captioned with the option's name, in a row */
export function OptionCheckbox({ name, checked = false }: { name: string; checked?: boolean }): ReactNode {
  return createElement(Node, { align_items: "Center", column_gap: px(5) }, createElement(FeathersCheckbox, { checked }, caption(name)));
}

/** helpers/scene.rs bottom_left_scene: the option menu's column, 10 px from the bottom left, its rows 6 px apart */
export function BottomLeft({ children }: { children?: ReactNode }): ReactNode {
  return createElement(Node, { flex_direction: "Column", position_type: "Absolute", row_gap: px(6), left: px(10), bottom: px(10) }, children);
}
