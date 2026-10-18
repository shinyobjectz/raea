use super::ElementCx;
use crate::color::{Color, ToColorColor as _};
use anyrender::PaintScene;
use kurbo::{Rect, Vec2};
use peniko::{Compose, Fill, Mix};

impl ElementCx<'_, '_> {
    /// raea: the area outer shadows and the outline paint outside the border box, in the
    /// element's local space (`None` when there are neither).
    pub(super) fn outset_paint_rect(&self) -> Option<Rect> {
        let mut rect: Option<Rect> = None;
        for shadow in self.style.get_effects().box_shadow.0.iter().filter(|s| !s.inset) {
            let x = shadow.base.horizontal.px() as f64 * self.scale;
            let y = shadow.base.vertical.px() as f64 * self.scale;
            let blur = shadow.base.blur.px() as f64 * self.scale;
            let spread = shadow.spread.px() as f64 * self.scale;
            let offset = spread.max(0.0) + blur * 2.5 + 1.0;
            let r = self.frame.border_box.inflate(offset, offset) + Vec2::new(x, y);
            rect = Some(rect.map_or(r, |acc| acc.union(r)));
        }
        let outline = self.style.get_outline();
        if outline.outline_width.0 .0 > 0 {
            let r = kurbo::Shape::bounding_box(&self.frame.outline()).inflate(1.0, 1.0);
            rect = Some(rect.map_or(r, |acc| acc.union(r)));
        }
        rect
    }

    pub(super) fn draw_outset_box_shadow(&self, scene: &mut impl PaintScene) {
        let box_shadow = &self.style.get_effects().box_shadow.0;
        let has_outset_shadow = box_shadow.iter().any(|s| !s.inset);
        if !has_outset_shadow {
            return;
        }

        let current_color = self.style.clone_color();
        let opacity = self.style.get_effects().opacity;
        let bg_color = self
            .style
            .get_background()
            .background_color
            .resolve_to_absolute(&current_color)
            .as_srgb_color();
        let bg_is_opaque = bg_color.components[3] >= 1.0;
        let needs_clip = opacity < 1.0 || !bg_is_opaque;

        let max_shadow_rect = box_shadow.iter().fold(Rect::ZERO, |prev, shadow| {
            let x = shadow.base.horizontal.px() as f64 * self.scale;
            let y = shadow.base.vertical.px() as f64 * self.scale;
            let blur = shadow.base.blur.px() as f64 * self.scale;
            let spread = shadow.spread.px() as f64 * self.scale;
            let offset = spread + blur * 2.5;

            let rect = self.frame.border_box.inflate(offset, offset) + Vec2::new(x, y);

            prev.union(rect)
        });

        self.context.layer_manager.maybe_with_layer(
            scene,
            needs_clip,
            1.0,
            self.transform,
            &self.frame.shadow_clip(max_shadow_rect),
            None,
            None,
            |scene| {
                for shadow in box_shadow.iter().filter(|s| !s.inset).rev() {
                    let shadow_color = shadow
                        .base
                        .color
                        .resolve_to_absolute(&current_color)
                        .as_srgb_color();

                    let alpha = shadow_color.components[3];
                    if alpha != 0.0 {
                        let transform = self.transform.then_translate(Vec2 {
                            x: shadow.base.horizontal.px() as f64 * self.scale,
                            y: shadow.base.vertical.px() as f64 * self.scale,
                        });

                        let spread = shadow.spread.px() as f64 * self.scale;
                        // raea: Gaussian sigma is half the CSS blur radius (Blink's
                        // BlurRadiusToStdDev), in device pixels
                        let blur = shadow.base.blur.px() as f64;
                        let sigma = blur * self.scale / 2.0;

                        // Without blur or spread the shadow is exactly the element's shape,
                        // so fill the border box path directly (preserving individual radii)
                        if blur == 0.0 && spread == 0.0 {
                            scene.fill(
                                Fill::NonZero,
                                transform,
                                shadow_color,
                                None,
                                &self.frame.border_box_path(),
                            );
                            continue;
                        }

                        // TODO draw shadows with matching individual radii instead of averaging
                        // (local change, raea) the spread grows the corner radius with the box, eased
                        // when the radius is smaller than the spread (CSS Backgrounds 3, Blink's
                        // OutsetForMarginOrShadow): a ring around a circle stays a circle
                        let r = self.frame.border_radii.average();
                        let radius = if spread > 0.0 && r > 0.0 {
                            if r >= spread { r + spread } else { r + spread * (1.0 + (r / spread - 1.0).powi(3)) }
                        } else if spread < 0.0 {
                            (r + spread).max(0.0)
                        } else {
                            r
                        };
                        let rect = self.frame.border_box.inflate(spread, spread);

                        // Fill the color
                        scene.draw_box_shadow(transform, rect, shadow_color, radius, sigma);
                    }
                }
            },
        )
    }

    pub(super) fn draw_inset_box_shadow(&self, scene: &mut impl PaintScene) {
        let current_color = self.style.clone_color();
        let box_shadow = &self.style.get_effects().box_shadow.0;
        let has_inset_shadow = box_shadow.iter().any(|s| s.inset);
        if !has_inset_shadow {
            return;
        }

        let padding_box = self.frame.padding_box_path();

        for shadow in box_shadow.iter().filter(|s| s.inset) {
            let shadow_color = shadow
                .base
                .color
                .resolve_to_absolute(&current_color)
                .as_srgb_color();
            if shadow_color == Color::TRANSPARENT {
                return;
            }

            // TODO draw shadows with matching individual radii instead of averaging
            let radius = self.frame.border_radii.average();
            let transform = self.transform.then_translate(Vec2 {
                x: shadow.base.horizontal.px() as f64,
                y: shadow.base.vertical.px() as f64,
            });

            scene.push_layer(
                Fill::NonZero,
                Mix::Normal,
                1.0,
                self.transform,
                &padding_box,
                None,
                None,
            );
            scene.fill(
                Fill::NonZero,
                self.transform,
                shadow_color,
                None,
                &padding_box,
            );

            scene.push_layer(
                Fill::NonZero,
                Compose::DestOut,
                1.0,
                self.transform,
                &padding_box,
                None,
                None,
            );
            // The unshadowed area is the padding box shrunk by the spread radius
            let spread = shadow.spread.px() as f64 * self.scale;
            let hole = self.frame.padding_box.inflate(-spread, -spread);
            scene.draw_box_shadow(
                transform,
                hole,
                Color::WHITE,
                radius,
                shadow.base.blur.px() as f64 * self.scale / 2.0,
            );

            scene.pop_layer();
            scene.pop_layer();
        }
    }
}
