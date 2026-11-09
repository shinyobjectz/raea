//! Text in the 2D world drawn by Bevy's own Text2d (`<Text2d>` inside a <Canvas3D>, sdk/render's bevy3d/ui.tsx): its
//! layout, glyphs and draw order among the sprites are bevy_text's and bevy_sprite_render's, so TextBounds,
//! FontSmoothing::None, a Text2dShadow, spans and a z under a sprite are as Bevy draws them. The row is a shape2d's
//! with a `text` column, JSON:
//!   { sections: [{ text, font (a font file's path, Bevy's default font when null), size, weight, color, background,
//!     underline, smoothing }, ...], justify, linebreak, bounds: [w, h] (either null), anchor: [x, y],
//!     shadow: { offset: [x, y], color } }
//! the first section the Text2d's own text and style, the rest its TextSpan children; colours sRGBA floats.

use std::collections::HashMap;

use bevy::prelude::*;
use bevy::sprite::{Anchor, Text2d, Text2dShadow};
use bevy::text::{Font, FontSmoothing, FontWeight, Justify, LineBreak, TextBackgroundColor, TextBounds, TextColor, TextFont, TextLayout, TextSpan, Underline};
use serde_json::Value;

/// Fonts read so far, by their file's path.
#[derive(Resource, Default)]
struct Fonts(HashMap<String, Handle<Font>>);

fn color(v: Option<&Value>) -> Option<Color> {
    let a = v?.as_array()?;
    let c = |i: usize| a.get(i).and_then(Value::as_f64).unwrap_or(1.0) as f32;
    Some(Color::srgba(c(0), c(1), c(2), c(3)))
}

fn pair(v: Option<&Value>) -> Option<Vec2> {
    let a = v?.as_array()?;
    Some(Vec2::new(a.first()?.as_f64()? as f32, a.get(1)?.as_f64()? as f32))
}

/// A section's TextFont, its font file read once.
fn font_of(world: &mut World, s: &Value) -> Result<TextFont, String> {
    let mut f = TextFont::from_font_size(s.get("size").and_then(Value::as_f64).unwrap_or(20.0) as f32);
    if let Some(path) = s.get("font").and_then(Value::as_str) {
        let known = world.get_resource_or_init::<Fonts>().0.get(path).cloned();
        let h = match known {
            Some(h) => h,
            None => {
                let bytes = std::fs::read(path).map_err(|e| format!("Text2d: font {path}: {e}"))?;
                let h = world.resource_mut::<Assets<Font>>().add(Font::from_bytes(bytes));
                world.resource_mut::<Fonts>().0.insert(path.to_string(), h.clone());
                h
            }
        };
        f.font = h.into();
    }
    if let Some(w) = s.get("weight").and_then(Value::as_u64) {
        f.weight = FontWeight(w as u16);
    }
    if s.get("smoothing").and_then(Value::as_str) == Some("None") {
        f.font_smoothing = FontSmoothing::None;
    }
    Ok(f)
}

/// A section's text, font, colour, background and underline, on `e`.
fn section(world: &mut World, e: Entity, s: &Value) -> Result<(), String> {
    let font = font_of(world, s)?;
    let mut ent = world.entity_mut(e);
    ent.insert((font, TextColor(color(s.get("color")).unwrap_or(Color::WHITE))));
    if let Some(b) = color(s.get("background")) {
        ent.insert(TextBackgroundColor(b));
    }
    if s.get("underline").and_then(Value::as_bool) == Some(true) {
        ent.insert(Underline);
    }
    Ok(())
}

/// The Text2d a row's `text` spec makes on `e` (its place, visibility and Part2d the caller's), its spans children.
pub fn spawn(world: &mut World, e: Entity, spec: &str) -> Result<(), String> {
    let spec: Value = serde_json::from_str(spec).map_err(|err| format!("Text2d: {err}"))?;
    let sections = spec.get("sections").and_then(Value::as_array).filter(|s| !s.is_empty()).ok_or("Text2d: no text")?;
    let justify = match spec.get("justify").and_then(Value::as_str).unwrap_or("Left") {
        "Center" => Justify::Center,
        "Right" => Justify::Right,
        "Justified" => Justify::Justified,
        "Start" => Justify::Start,
        "End" => Justify::End,
        _ => Justify::Left,
    };
    let linebreak = match spec.get("linebreak").and_then(Value::as_str).unwrap_or("WordBoundary") {
        "AnyCharacter" => LineBreak::AnyCharacter,
        "WordOrCharacter" => LineBreak::WordOrCharacter,
        "NoWrap" => LineBreak::NoWrap,
        _ => LineBreak::WordBoundary,
    };
    let text = |s: &Value| s.get("text").and_then(Value::as_str).unwrap_or("").to_string();
    world.entity_mut(e).insert((Text2d::new(text(&sections[0])), TextLayout::new(justify, linebreak), Anchor(pair(spec.get("anchor")).unwrap_or(Vec2::ZERO))));
    section(world, e, &sections[0])?;
    if let Some(b) = spec.get("bounds").and_then(Value::as_array) {
        let side = |i: usize| b.get(i).and_then(Value::as_f64).map(|x| x as f32);
        world.entity_mut(e).insert(TextBounds { width: side(0), height: side(1) });
    }
    if let Some(s) = spec.get("shadow").filter(|s| s.is_object()) {
        let d = Text2dShadow::default();
        world.entity_mut(e).insert(Text2dShadow { offset: pair(s.get("offset")).unwrap_or(d.offset), color: color(s.get("color")).unwrap_or(d.color) });
    }
    for s in &sections[1..] {
        let span = world.spawn(TextSpan::new(text(s))).id();
        section(world, span, s)?;
        world.entity_mut(e).add_child(span);
    }
    Ok(())
}
