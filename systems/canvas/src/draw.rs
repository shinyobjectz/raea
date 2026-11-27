//! The display list's ops drawn: fills, strokes, clears, text, images and videos, each under its state's shadow,
//! alpha, blend and clip.

use super::*;

impl Ctx<'_> {
    pub(crate) fn op(&mut self, pm: &mut sk::Pixmap, op: &Value) {
        let Some(a) = op.as_array() else { return };
        let (w, h) = (pm.width(), pm.height());
        match a.first().and_then(Value::as_str) {
            Some("clear") => {
                let st = &a[2];
                let Some(p) = path_of(&a[1]) else { return };
                let mask = clip_of(st, w, h);
                let mut paint = sk::Paint::default();
                paint.set_color(sk::Color::TRANSPARENT);
                paint.blend_mode = sk::BlendMode::Clear;
                paint.anti_alias = false;
                pm.fill_path(&p, &paint, sk::FillRule::Winding, sk::Transform::identity(), mask.as_ref());
            }
            Some("fill") => {
                let st = &a[4];
                let Some(p) = path_of(&a[1]) else { return };
                let rule = if a[2].as_str() == Some("evenodd") { sk::FillRule::EvenOdd } else { sk::FillRule::Winding };
                let alpha = st["alpha"].as_f64().unwrap_or(1.0) as f32;
                // in the call's own space, so a gradient is in it too
                let ts = ts_of(&st["ts"]);
                let Some(inv) = ts.invert() else { return };
                let Some(local) = p.transform(inv) else { return };
                self.shape(pm, st, |layer, paint| {
                    let mut paint = paint.clone();
                    if let Some(s) = shader_of(&a[3], alpha) {
                        paint.shader = s;
                    }
                    layer.fill_path(&local, &paint, rule, ts, None);
                });
            }
            Some("stroke") => {
                let st = &a[4];
                let Some(p) = path_of(&a[1]) else { return };
                let alpha = st["alpha"].as_f64().unwrap_or(1.0) as f32;
                let ts = ts_of(&st["ts"]);
                let Some(inv) = ts.invert() else { return };
                let Some(local) = p.transform(inv) else { return };
                let stroke = stroke_of(&a[2]);
                self.shape(pm, st, |layer, paint| {
                    let mut paint = paint.clone();
                    if let Some(s) = shader_of(&a[3], alpha) {
                        paint.shader = s;
                    }
                    layer.stroke_path(&local, &paint, &stroke, ts, None);
                });
            }
            Some("text") => self.text(pm, a),
            Some("image") => {
                let Some(img) = a.get(1).and_then(Value::as_str).and_then(|src| self.decode(src)) else { return };
                self.image(pm, a, &img)
            }
            // ["canvas", {key, w, h, ops}, sx, sy, sw, sh, dx, dy, dw, dh, state]: another canvas drawn on this one, its
            // list as it was then, replayed here (Excalidraw draws each element on a canvas of its own, then those)
            Some("canvas") => {
                let Some(img) = a.get(1).and_then(|l| self.nested(l)) else { return };
                self.image(pm, a, &img)
            }
            // ["video", src, sx, sy, sw, sh, dx, dy, dw, dh, state, t]: the frame showing at t
            Some("video") => {
                let Some(img) = a.get(1).and_then(Value::as_str).and_then(|src| video_frame(src, a.get(11).and_then(Value::as_f64).unwrap_or(0.0))) else { return };
                self.image(pm, a, &img)
            }
            // ["put", {w, h, data}, dx, dy, x, y, w, h]: putImageData, the image's RGBA (base64) written as it is, with
            // no transform, alpha, compositing or clip, its dirty rectangle (x, y, w, h in the image) only (LR6)
            Some("put") => {
                use base64::Engine as _;
                let (iw, ih) = (a[1]["w"].as_i64().unwrap_or(0), a[1]["h"].as_i64().unwrap_or(0));
                let data = a[1]["data"].as_str().and_then(|d| base64::engine::general_purpose::STANDARD.decode(d).ok());
                let Some(px) = data.filter(|p| p.len() as i64 == iw * ih * 4) else { return };
                let n = |i: usize| a.get(i).and_then(Value::as_f64).unwrap_or(0.0) as i64;
                let (dx, dy) = (n(2), n(3));
                let (x0, y0, x1, y1) = (n(4).max(0), n(5).max(0), (n(4) + n(6)).min(iw), (n(5) + n(7)).min(ih));
                let (w, h) = (i64::from(w), i64::from(h));
                let out = pm.pixels_mut();
                for y in (y0..y1).filter(|y| (0..h).contains(&(dy + y))) {
                    for x in (x0..x1).filter(|x| (0..w).contains(&(dx + x))) {
                        let s = ((y * iw + x) * 4) as usize;
                        out[((dy + y) * w + dx + x) as usize] = sk::ColorU8::from_rgba(px[s], px[s + 1], px[s + 2], px[s + 3]).premultiply();
                    }
                }
            }
            _ => {}
        }
    }

    /// A shape drawn through the call's clip, alpha, compositing and shadow: `draw` paints it on a layer (or straight
    /// onto the bitmap when nothing needs a layer).
    pub(crate) fn shape(&mut self, pm: &mut sk::Pixmap, st: &Value, draw: impl Fn(&mut sk::Pixmap, &sk::Paint)) {
        let (w, h) = (pm.width(), pm.height());
        let mut paint = sk::Paint { anti_alias: true, ..Default::default() };
        let mode = blend(st["comp"].as_str().unwrap_or("source-over"));
        let clip = clip_of(st, w, h);
        let shadow = st.get("shadow").filter(|s| !s.is_null());
        let filters = st.get("filter").and_then(Value::as_array).filter(|l| !l.is_empty());
        if shadow.is_none() && filters.is_none() && clip.is_none() && matches!(mode, sk::BlendMode::SourceOver | sk::BlendMode::DestinationOut | sk::BlendMode::DestinationOver | sk::BlendMode::Plus | sk::BlendMode::Multiply | sk::BlendMode::Screen) {
            paint.blend_mode = mode;
            draw(pm, &paint);
            return;
        }
        let Some(mut layer) = sk::Pixmap::new(w, h) else { return };
        draw(&mut layer, &paint);
        if let Some(l) = filters {
            crate::filter::apply(&mut layer, l);
        }
        if let Some(sh) = shadow {
            let Some(mut s) = sk::Pixmap::new(w, h) else { return };
            let col = color_of(&sh["color"], 1.0).unwrap_or(sk::Color::BLACK);
            let (ox, oy) = (f(&sh["x"]), f(&sh["y"]));
            // the shape's alpha in the shadow's colour, offset, blurred (sigma: half the blur)
            if let Some(mask) = Some(sk::Mask::from_pixmap(layer.as_ref(), sk::MaskType::Alpha)) {
                let mut p = sk::Paint::default();
                p.set_color(col);
                s.fill_rect(sk::Rect::from_xywh(0.0, 0.0, w as f32, h as f32).unwrap(), &p, sk::Transform::identity(), Some(&mask));
            }
            let mut shifted = sk::Pixmap::new(w, h).unwrap();
            shifted.draw_pixmap(0, 0, s.as_ref(), &sk::PixmapPaint::default(), sk::Transform::from_translate(ox, oy), None);
            blur(&mut shifted, f(&sh["blur"]) / 2.0);
            pm.draw_pixmap(0, 0, shifted.as_ref(), &sk::PixmapPaint { blend_mode: mode, ..Default::default() }, sk::Transform::identity(), clip.as_ref());
        }
        pm.draw_pixmap(0, 0, layer.as_ref(), &sk::PixmapPaint { blend_mode: mode, ..Default::default() }, sk::Transform::identity(), clip.as_ref());
    }

    pub(crate) fn text(&mut self, pm: &mut sk::Pixmap, a: &[Value]) {
        // ["text", kind, text, x, y, font, align, baseline, maxWidth, paint, stroke, state]
        let (Some(kind), Some(text)) = (a.get(1).and_then(Value::as_str), a.get(2).and_then(Value::as_str)) else { return };
        let st = &a[11];
        let paint_attr = if kind == "fill" {
            "fill=\"#fff\"".to_string()
        } else {
            let s = &a[10];
            format!("fill=\"none\" stroke=\"#fff\" stroke-width=\"{}\" stroke-linejoin=\"{}\"", f(&s["width"]), s["join"].as_str().unwrap_or("miter"))
        };
        let (w, h) = (pm.width(), pm.height());
        let doc = crate::measure::text_doc(self.opt, &a[5], text, (f(&a[3]), f(&a[4])), (&a[6], &a[7]), &paint_attr, (w, h));
        let Ok(tree) = usvg::Tree::from_str(&doc, self.opt) else { return };
        let ts = ts_of(&st["ts"]);
        // the glyphs' coverage in the call's space, then the paint through it
        let Some(mut cover) = sk::Pixmap::new(w, h) else { return };
        resvg::render(&tree, ts, &mut cover.as_mut());
        let mask = sk::Mask::from_pixmap(cover.as_ref(), sk::MaskType::Alpha);
        let alpha = st["alpha"].as_f64().unwrap_or(1.0) as f32;
        let shader = shader_of(&a[9], alpha);
        let Some(inv) = ts.invert() else { return };
        self.shape(pm, st, |layer, paint| {
            let mut paint = paint.clone();
            if let Some(s) = shader.clone() {
                paint.shader = s;
            }
            // a rect over the whole bitmap in the call's space, painted through the glyphs
            let r = sk::Rect::from_xywh(0.0, 0.0, w as f32, h as f32).unwrap();
            if let Some(p) = sk::PathBuilder::from_rect(r).transform(inv) {
                layer.fill_path(&p, &paint, sk::FillRule::Winding, ts, Some(&mask));
            }
        });
    }

    pub(crate) fn decode(&mut self, src: &str) -> Option<sk::Pixmap> {
        if let Some(p) = self.images.get(src) {
            return Some(p.clone());
        }
        // inline (data:), else the picture's file (a page's /media/: the host's roots, as its videos)
        let (mime, bytes) = raea_io_net::data_url(src).or_else(|| crate::image_file(src))?;
        let mime = mime.as_str();
        let img = if mime == "image/svg+xml" {
            let size = crate::svg::natural_size(&bytes, self.opt)?;
            crate::svg::raster_doc(&bytes, size.as_uvec2().max(UVec2::ONE), self.opt)?
        } else {
            Image::from_buffer(&bytes, bevy::image::ImageType::MimeType(mime), bevy::image::CompressedImageFormats::NONE, true, bevy::image::ImageSampler::Default, RenderAssetUsages::RENDER_WORLD).ok()?
        };
        let img = img.convert(TextureFormat::Rgba8UnormSrgb)?;
        let (w, h) = (img.width(), img.height());
        let data = img.data?;
        let mut pm = sk::Pixmap::new(w, h)?;
        for (i, px) in pm.pixels_mut().iter_mut().enumerate() {
            let c = &data[i * 4..i * 4 + 4];
            *px = sk::ColorU8::from_rgba(c[0], c[1], c[2], c[3]).premultiply();
        }
        self.images.insert(src.to_string(), pm.clone());
        Some(pm)
    }

    /// A canvas's list drawn on a bitmap of its own size: kept by its key (the page gives one per canvas, list
    /// generation and length), as the same element canvas is drawn on the board frame after frame.
    fn nested(&mut self, list: &Value) -> Option<sk::Pixmap> {
        static DRAWN: std::sync::Mutex<Vec<(String, sk::Pixmap)>> = std::sync::Mutex::new(Vec::new());
        let key = list["key"].as_str().map(str::to_string);
        if let Some(k) = &key
            && let Some((_, pm)) = DRAWN.lock().unwrap().iter().find(|(x, _)| x == k)
        {
            return Some(pm.clone());
        }
        let n = |k: &str| list[k].as_u64().filter(|v| *v > 0 && *v <= 16384).map(|v| v as u32);
        let mut out = sk::Pixmap::new(n("w")?, n("h")?)?;
        for op in list["ops"].as_array().into_iter().flatten() {
            self.op(&mut out, op);
        }
        if let Some(k) = key {
            let mut drawn = DRAWN.lock().unwrap();
            drawn.push((k, out.clone()));
            // the newest kept, up to 256 MB
            while drawn.iter().map(|(_, p)| p.data().len()).sum::<usize>() > 256 << 20 && drawn.len() > 1 {
                drawn.remove(0);
            }
        }
        Some(out)
    }

    pub(crate) fn image(&mut self, pm: &mut sk::Pixmap, a: &[Value], img: &sk::Pixmap) {
        // ["image", src, sx, sy, sw, sh, dx, dy, dw, dh, state]
        let n = |i: usize| a.get(i).and_then(Value::as_f64).map(|v| v as f32);
        let (iw, ih) = (img.width() as f32, img.height() as f32);
        let (sx, sy, sw, sh) = (n(2).unwrap_or(0.0), n(3).unwrap_or(0.0), n(4).unwrap_or(iw), n(5).unwrap_or(ih));
        let (dx, dy) = (n(6).unwrap_or(0.0), n(7).unwrap_or(0.0));
        let (dw, dh) = (n(8).unwrap_or(sw), n(9).unwrap_or(sh));
        if sw == 0.0 || sh == 0.0 {
            return;
        }
        let st = &a[10];
        let ts = ts_of(&st["ts"]);
        let alpha = st["alpha"].as_f64().unwrap_or(1.0) as f32;
        let smooth = st["smooth"].as_bool().unwrap_or(true);
        // the source rect mapped onto the destination, in the call's space
        // in the call's own space: fill_rect below applies `ts` to the shader too (placed with ts here as well, an
        // image under a scale drew from the wrong spot, its transparent padding, and Excalidraw's boards came out blank)
        // drawn under half size, smoothed: sampled from a mip level (halved), as Chrome does
        let shrink = ((ts.sx * ts.sx + ts.ky * ts.ky).sqrt() * dw / sw).abs().min(((ts.kx * ts.kx + ts.sy * ts.sy).sqrt() * dh / sh).abs());
        let levels = if smooth && shrink > 0.0 && shrink < 0.5 { ((1.0 / shrink).log2().floor() as u32).min(8) } else { 0 };
        let mip = (levels > 0).then(|| halved(img, levels)).flatten();
        let (img, k) = match &mip {
            Some(m) => (m, iw / m.width() as f32),
            None => (img, 1.0),
        };
        let place = sk::Transform::from_translate(dx, dy).pre_scale(dw / sw * k, dh / sh * k).pre_translate(-sx / k, -sy / k);
        let quality = if smooth { sk::FilterQuality::Bilinear } else { sk::FilterQuality::Nearest };
        let pattern = sk::Pattern::new(img.as_ref(), sk::SpreadMode::Pad, quality, alpha, place);
        let Some(inv) = ts.invert() else { return };
        let _ = inv;
        self.shape(pm, st, |layer, paint| {
            let mut paint = paint.clone();
            paint.shader = pattern.clone();
            if let Some(r) = sk::Rect::from_xywh(dx, dy, dw, dh) {
                layer.fill_rect(r, &paint, ts, None);
            }
        });
    }
}

/// `img` halved `levels` times, each pixel the mean of the 2x2 block under it (premultiplied, so a mean is right): the
/// mip level Chrome samples an image drawn small from. Bilinear on the full image aliases, and a poster's stars
/// sparkled on a board at 50%.
fn halved(img: &sk::Pixmap, levels: u32) -> Option<sk::Pixmap> {
    let mut cur: std::borrow::Cow<sk::Pixmap> = std::borrow::Cow::Borrowed(img);
    for _ in 0..levels {
        let (w, h) = (cur.width(), cur.height());
        if w < 2 || h < 2 {
            break;
        }
        let (nw, nh) = (w / 2, h / 2);
        let mut out = sk::Pixmap::new(nw, nh)?;
        let src = cur.data();
        let dst = out.data_mut();
        for y in 0..nh as usize {
            for x in 0..nw as usize {
                for c in 0..4 {
                    let at = |yy: usize, xx: usize| u16::from(src[(yy * w as usize + xx) * 4 + c]);
                    let sum = at(2 * y, 2 * x) + at(2 * y, 2 * x + 1) + at(2 * y + 1, 2 * x) + at(2 * y + 1, 2 * x + 1);
                    dst[(y * nw as usize + x) * 4 + c] = ((sum + 2) / 4) as u8;
                }
            }
        }
        cur = std::borrow::Cow::Owned(out);
    }
    match cur {
        std::borrow::Cow::Owned(p) => Some(p),
        std::borrow::Cow::Borrowed(_) => None,
    }
}


#[cfg(test)]
mod tests {
    #[test]
    fn a_checkerboard_halved_is_its_mean() {
        let mut pm = tiny_skia::Pixmap::new(4, 4).unwrap();
        for (i, px) in pm.data_mut().chunks_mut(4).enumerate() {
            let on = (i % 4 + i / 4) % 2 == 0;
            px.copy_from_slice(if on { &[255, 255, 255, 255] } else { &[0, 0, 0, 255] });
        }
        let h = super::halved(&pm, 1).unwrap();
        assert_eq!((h.width(), h.height()), (2, 2));
        assert!(h.data().chunks(4).all(|p| p == [128, 128, 128, 255]), "{:?}", h.data());
        assert!(super::halved(&pm, 0).is_none());
    }
}
