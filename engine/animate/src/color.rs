//! Colour (after animato-color 1.7.1, which wraps palette): sRGB colours parsed from CSS text and blended in Oklab
//! (perceptually even, the default), Oklch (around the hue wheel) or CIE Lab, with premultiplied alpha as CSS
//! color-mix() blends. Written out from the published conversion matrices (Björn Ottosson's Oklab, CIE D65), with no
//! palette dependency.

use crate::interpolate::Interpolate;

/// An sRGB colour, components and alpha in [0, 1] (straight alpha).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

/// Which space a blend happens in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Space {
    #[default]
    Oklab,
    Oklch,
    Lab,
    Srgb,
}

const NAMED: &[(&str, [u8; 3])] = &[
    ("black", [0, 0, 0]), ("white", [255, 255, 255]), ("red", [255, 0, 0]), ("green", [0, 128, 0]), ("blue", [0, 0, 255]),
    ("yellow", [255, 255, 0]), ("orange", [255, 165, 0]), ("purple", [128, 0, 128]), ("gray", [128, 128, 128]),
    ("grey", [128, 128, 128]), ("pink", [255, 192, 203]), ("cyan", [0, 255, 255]), ("magenta", [255, 0, 255]),
    ("gold", [255, 215, 0]), ("silver", [192, 192, 192]), ("navy", [0, 0, 128]), ("teal", [0, 128, 128]),
];

fn to_linear(c: f64) -> f64 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}
fn to_gamma(c: f64) -> f64 {
    if c <= 0.003_130_8 { 12.92 * c } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}

impl Color {
    pub const TRANSPARENT: Color = Color { r: 0.0, g: 0.0, b: 0.0, a: 0.0 };

    pub fn rgba(r: f64, g: f64, b: f64, a: f64) -> Color {
        Color { r, g, b, a }
    }

    /// A colour from CSS text: `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`, `rgb()`/`rgba()` (numbers or percentages,
    /// commas or spaces), `transparent` or a common name. None for anything else (a `var(...)`, a gradient).
    pub fn parse(s: &str) -> Option<Color> {
        let s = s.trim().to_ascii_lowercase();
        if s == "transparent" {
            return Some(Color::TRANSPARENT);
        }
        if let Some(h) = s.strip_prefix('#') {
            let d = |i: usize, n: usize| u8::from_str_radix(&h[i..i + n], 16).ok().map(|v| if n == 1 { v * 17 } else { v } as f64 / 255.0);
            return match h.len() {
                3 => Some(Color::rgba(d(0, 1)?, d(1, 1)?, d(2, 1)?, 1.0)),
                4 => Some(Color::rgba(d(0, 1)?, d(1, 1)?, d(2, 1)?, d(3, 1)?)),
                6 => Some(Color::rgba(d(0, 2)?, d(2, 2)?, d(4, 2)?, 1.0)),
                8 => Some(Color::rgba(d(0, 2)?, d(2, 2)?, d(4, 2)?, d(6, 2)?)),
                _ => None,
            };
        }
        if let Some(inner) = s.strip_prefix("rgba(").or_else(|| s.strip_prefix("rgb(")).and_then(|r| r.strip_suffix(')')) {
            let parts: Vec<&str> = inner.split(|c: char| c == ',' || c == '/' || c.is_whitespace()).filter(|p| !p.is_empty()).collect();
            if parts.len() < 3 {
                return None;
            }
            let chan = |p: &str| -> Option<f64> { if let Some(x) = p.strip_suffix('%') { x.parse::<f64>().ok().map(|v| v / 100.0) } else { p.parse::<f64>().ok().map(|v| v / 255.0) } };
            let a = match parts.get(3) {
                Some(p) => if let Some(x) = p.strip_suffix('%') { x.parse::<f64>().ok()? / 100.0 } else { p.parse::<f64>().ok()? },
                None => 1.0,
            };
            return Some(Color::rgba(chan(parts[0])?, chan(parts[1])?, chan(parts[2])?, a.clamp(0.0, 1.0)));
        }
        NAMED.iter().find(|(n, _)| *n == s).map(|(_, c)| Color::rgba(c[0] as f64 / 255.0, c[1] as f64 / 255.0, c[2] as f64 / 255.0, 1.0))
    }

    /// Oklab [L, a, b].
    pub fn to_oklab(&self) -> [f64; 3] {
        let (r, g, b) = (to_linear(self.r), to_linear(self.g), to_linear(self.b));
        let l = (0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b).cbrt();
        let m = (0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b).cbrt();
        let s = (0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b).cbrt();
        [
            0.210_454_255_3 * l + 0.793_617_785_0 * m - 0.004_072_046_8 * s,
            1.977_998_495_1 * l - 2.428_592_205_0 * m + 0.450_593_709_9 * s,
            0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766_0 * s,
        ]
    }

    pub fn from_oklab(lab: [f64; 3], a: f64) -> Color {
        let l = lab[0] + 0.396_337_777_4 * lab[1] + 0.215_803_757_3 * lab[2];
        let m = lab[0] - 0.105_561_345_8 * lab[1] - 0.063_854_172_8 * lab[2];
        let s = lab[0] - 0.089_484_177_5 * lab[1] - 1.291_485_548_0 * lab[2];
        let (l, m, s) = (l * l * l, m * m * m, s * s * s);
        let r = 4.076_741_662_1 * l - 3.307_711_591_3 * m + 0.230_969_929_2 * s;
        let g = -1.268_438_004_6 * l + 2.609_757_401_1 * m - 0.341_319_396_5 * s;
        let b = -0.004_196_086_3 * l - 0.703_418_614_7 * m + 1.707_614_701_0 * s;
        Color::rgba(to_gamma(r).clamp(0.0, 1.0), to_gamma(g).clamp(0.0, 1.0), to_gamma(b).clamp(0.0, 1.0), a)
    }

    /// Oklch [L, C, h (degrees)].
    pub fn to_oklch(&self) -> [f64; 3] {
        let [l, a, b] = self.to_oklab();
        [l, (a * a + b * b).sqrt(), b.atan2(a).to_degrees().rem_euclid(360.0)]
    }

    pub fn from_oklch(lch: [f64; 3], alpha: f64) -> Color {
        let h = lch[2].to_radians();
        Color::from_oklab([lch[0], lch[1] * h.cos(), lch[1] * h.sin()], alpha)
    }

    /// CIE Lab (D65) [L, a, b].
    pub fn to_lab(&self) -> [f64; 3] {
        let (r, g, b) = (to_linear(self.r), to_linear(self.g), to_linear(self.b));
        let x = (0.412_456_4 * r + 0.357_576_1 * g + 0.180_437_5 * b) / 0.950_47;
        let y = 0.212_672_9 * r + 0.715_152_2 * g + 0.072_175_0 * b;
        let z = (0.019_333_9 * r + 0.119_192_0 * g + 0.950_304_1 * b) / 1.088_83;
        let f = |t: f64| if t > 216.0 / 24389.0 { t.cbrt() } else { (24389.0 / 27.0 * t + 16.0) / 116.0 };
        let (fx, fy, fz) = (f(x), f(y), f(z));
        [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
    }

    pub fn from_lab(lab: [f64; 3], alpha: f64) -> Color {
        let fy = (lab[0] + 16.0) / 116.0;
        let fx = fy + lab[1] / 500.0;
        let fz = fy - lab[2] / 200.0;
        let finv = |t: f64| if t * t * t > 216.0 / 24389.0 { t * t * t } else { (116.0 * t - 16.0) / (24389.0 / 27.0) };
        let (x, y, z) = (finv(fx) * 0.950_47, finv(fy), finv(fz) * 1.088_83);
        let r = 3.240_454_2 * x - 1.537_138_5 * y - 0.498_531_4 * z;
        let g = -0.969_266_0 * x + 1.876_010_8 * y + 0.041_556_0 * z;
        let b = 0.055_643_4 * x - 0.204_025_9 * y + 1.057_225_2 * z;
        Color::rgba(to_gamma(r).clamp(0.0, 1.0), to_gamma(g).clamp(0.0, 1.0), to_gamma(b).clamp(0.0, 1.0), alpha)
    }

    /// Blended toward `other` by `t` in `space`, premultiplied (a fade from transparent doesn't pass through grey).
    pub fn mix(&self, other: &Color, t: f64, space: Space) -> Color {
        if t == 0.0 {
            return *self;
        }
        if t == 1.0 {
            return *other;
        }
        let a = self.a.lerp(&other.a, t);
        let pre = |c: [f64; 3], al: f64| [c[0] * al, c[1] * al, c[2] * al];
        let un = |c: [f64; 3]| if a > 0.0 { [c[0] / a, c[1] / a, c[2] / a] } else { c };
        match space {
            Space::Oklab => Color::from_oklab(un(pre(self.to_oklab(), self.a).lerp(&pre(other.to_oklab(), other.a), t)), a),
            Space::Lab => Color::from_lab(un(pre(self.to_lab(), self.a).lerp(&pre(other.to_lab(), other.a), t)), a),
            Space::Srgb => {
                let c = un(pre([self.r, self.g, self.b], self.a).lerp(&pre([other.r, other.g, other.b], other.a), t));
                Color::rgba(c[0].clamp(0.0, 1.0), c[1].clamp(0.0, 1.0), c[2].clamp(0.0, 1.0), a)
            }
            Space::Oklch => {
                let (p, q) = (self.to_oklch(), other.to_oklch());
                // the shorter way round the hue wheel; an achromatic end takes the other's hue
                let (mut h1, mut h2) = (p[2], q[2]);
                if p[1] < 1e-6 {
                    h1 = h2;
                }
                if q[1] < 1e-6 {
                    h2 = h1;
                }
                let mut d = h2 - h1;
                if d > 180.0 {
                    d -= 360.0;
                } else if d < -180.0 {
                    d += 360.0;
                }
                Color::from_oklch([p[0].lerp(&q[0], t), p[1].lerp(&q[1], t), (h1 + d * t).rem_euclid(360.0)], a)
            }
        }
    }

    /// CSS text: `rgba(r, g, b, a)` with 8-bit channels, or `#rrggbb` when `hex` (alpha dropped; the 3D renderer's form).
    pub fn css(&self, hex: bool) -> String {
        let q = |c: f64| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
        if hex {
            format!("#{:02x}{:02x}{:02x}", q(self.r), q(self.g), q(self.b))
        } else {
            format!("rgba({}, {}, {}, {})", q(self.r), q(self.g), q(self.b), (self.a.clamp(0.0, 1.0) * 1000.0).round() / 1000.0)
        }
    }
}

impl Interpolate for Color {
    fn lerp(&self, other: &Self, t: f64) -> Self {
        self.mix(other, t, Space::Oklab)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_css() {
        assert_eq!(Color::parse("#ff0000"), Some(Color::rgba(1.0, 0.0, 0.0, 1.0)));
        assert_eq!(Color::parse("#f00f").unwrap().a, 1.0);
        assert_eq!(Color::parse("rgba(0, 0, 255, 0.5)"), Some(Color::rgba(0.0, 0.0, 1.0, 0.5)));
        assert_eq!(Color::parse("rgb(0 0 255 / 50%)").unwrap().a, 0.5);
        assert!(Color::parse("var(--brand-accent)").is_none());
    }

    #[test]
    fn round_trips_through_each_space() {
        for c in [Color::rgba(0.2, 0.5, 0.9, 1.0), Color::rgba(1.0, 0.8, 0.1, 1.0), Color::rgba(0.0, 0.0, 0.0, 1.0)] {
            let o = Color::from_oklab(c.to_oklab(), 1.0);
            let l = Color::from_lab(c.to_lab(), 1.0);
            let h = Color::from_oklch(c.to_oklch(), 1.0);
            for d in [o, l, h] {
                assert!((d.r - c.r).abs() < 1e-4 && (d.g - c.g).abs() < 1e-4 && (d.b - c.b).abs() < 1e-4, "{c:?} -> {d:?}");
            }
        }
    }

    #[test]
    fn mix_is_exact_at_the_ends_and_premultiplied() {
        let (a, b) = (Color::parse("#000000").unwrap(), Color::parse("#ffffff").unwrap());
        assert_eq!(a.mix(&b, 0.0, Space::Oklab), a);
        assert_eq!(a.mix(&b, 1.0, Space::Oklab), b);
        // oklab's midpoint of black and white is lighter than sRGB's
        assert!(a.mix(&b, 0.5, Space::Oklab).r > 0.35);
        let red = Color::parse("red").unwrap();
        let half = Color::TRANSPARENT.mix(&red, 0.5, Space::Oklab);
        assert!((half.a - 0.5).abs() < 1e-9 && half.r > 0.99, "{half:?}");
    }
}
