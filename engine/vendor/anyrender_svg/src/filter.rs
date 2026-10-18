//! raea: usvg filters as anyrender filter graphs, following Blink's SVGFilterBuilder.
//!
//! Used for filters inside an SVG and by blitz-paint for `filter: url(#id)` on HTML elements.
//!
//! - Every primitive keeps its subregion (usvg resolves `x`/`y`/`width`/`height` and the units), cropped
//!   to the filter region as Blink's `FilterEffect::GetCropRect` does.
//! - `color-interpolation-filters` is kept per primitive (linearRGB by default); the rasterizer converts
//!   inputs and results between sRGB and linearRGB where the spaces differ, like Blink's
//!   `TransformInterpolationSpace`.
//! - `SourceAlpha` is an alpha-only colour matrix over the source (Blink's `SourceAlpha`).
//! - `feMerge` is a chain of source-over composites (Skia's merge filter draws its inputs source-over).
//! - A filter list (`filter="url(#a) url(#b)"`) chains: each filter's SourceGraphic is the previous
//!   filter's result.

use std::collections::HashMap;
use std::sync::Arc;

use anyrender::filters::{
    BlendMode, EdgeMode, ExternalImageSource, Filter, FilterEffect, FilterId, FilterInput, FilterInputs, FilterSource,
    color_transformation::ColorMatrix,
    component_transfer::{ComponentTransferFilter, GammaTransferFunction, LinearTransferFunction, TransferFunction},
    composite::{ArithmeticCompositeOperator, CompositeOperator},
    convolution::ConvolutionKernel,
    displacement::{ColorChannel, DisplacementMapFilter},
    lighting::{
        DiffuseLightingFilter, DistantLightSource, LightSource, PointLightSource, SpecularLightingFilter,
        SpotLightSource,
    },
    morphology::{MorphologyFilter, MorphologyOperator},
    shadow::DropShadow,
    turbulence::{TurbulenceFilter, TurbulenceType},
    blur::GaussianBlurFilter,
};
use kurbo::{Rect, Vec2};
use peniko::color::{AlphaColor, Srgb};
use usvg::filter as uf;

/// One graph for an element's filter list, or `None` when the list is empty.
pub fn convert_filters(filters: &[Arc<uf::Filter>]) -> Option<Filter> {
    if filters.is_empty() {
        return None;
    }
    let mut b = Builder { graph: Filter::empty(), source: None };
    let mut region = None;
    for f in filters {
        region = Some(b.add_filter(f));
    }
    b.graph.set_region(region);
    Some(b.graph)
}

/// The union of the filters' regions (user space): where a filtered element can draw.
pub fn filters_region(filters: &[Arc<uf::Filter>]) -> Option<Rect> {
    filters.iter().map(|f| to_rect(f.rect())).reduce(|a, b| a.union(b))
}

struct Builder {
    graph: Filter,
    /// The previous filter's result: the next filter's SourceGraphic.
    source: Option<FilterId>,
}

impl Builder {
    /// Adds one `<filter>`'s primitives; returns its region.
    fn add_filter(&mut self, f: &uf::Filter) -> Rect {
        let region = to_rect(f.rect());
        let mut names: HashMap<&str, (FilterId, Rect)> = HashMap::new();
        let mut source_alpha: Option<FilterId> = None;
        let mut last: Option<FilterId> = None;
        for p in f.primitives() {
            let sub = to_rect(p.rect()).intersect(region);
            let linear = matches!(p.color_interpolation(), uf::ColorInterpolation::LinearRGB);
            let source = self.source;
            let mut input = |graph: &mut Filter, i: &uf::Input| -> Option<FilterInput> {
                let src = |s: Option<FilterId>| match s {
                    Some(id) => FilterInput::Result(id),
                    None => FilterInput::Source(FilterSource::SourceGraphic),
                };
                Some(match i {
                    uf::Input::SourceGraphic => src(source),
                    uf::Input::SourceAlpha => {
                        let id = *source_alpha.get_or_insert_with(|| {
                            let mut m = [0.0; 20];
                            m[18] = 1.0;
                            graph.add_svg(
                                FilterEffect::ColorMatrix(ColorMatrix(m)),
                                FilterInputs { primary: Some(src(source)), secondary: None },
                                None,
                                false,
                            )
                        });
                        FilterInput::Result(id)
                    }
                    uf::Input::Reference(name) => match names.get(name.as_str()) {
                        Some((id, _)) => FilterInput::Result(*id),
                        None => src(source),
                    },
                })
            };
            let one = |graph: &mut Filter, i: &uf::Input, input: &mut dyn FnMut(&mut Filter, &uf::Input) -> Option<FilterInput>| {
                FilterInputs { primary: input(graph, i), secondary: None }
            };
            let g = &mut self.graph;
            let id = match p.kind() {
                uf::Kind::Blend(k) => {
                    let inputs = FilterInputs { primary: input(g, k.input1()), secondary: input(g, k.input2()) };
                    g.add_svg(FilterEffect::Blend(to_mix(k.mode())), inputs, Some(sub), linear)
                }
                uf::Kind::ColorMatrix(k) => {
                    let m = match k.kind() {
                        uf::ColorMatrixKind::Matrix(v) => {
                            let mut m = [0.0; 20];
                            m.iter_mut().zip(v.iter()).for_each(|(d, s)| *d = *s);
                            m
                        }
                        uf::ColorMatrixKind::Saturate(s) => saturate_matrix(s.get()),
                        uf::ColorMatrixKind::HueRotate(deg) => hue_rotate_matrix(*deg),
                        uf::ColorMatrixKind::LuminanceToAlpha => {
                            let mut m = [0.0; 20];
                            m[15] = 0.2125;
                            m[16] = 0.7154;
                            m[17] = 0.0721;
                            m
                        }
                    };
                    let inputs = one(g, k.input(), &mut input);
                    g.add_svg(FilterEffect::ColorMatrix(ColorMatrix(m)), inputs, Some(sub), linear)
                }
                uf::Kind::ComponentTransfer(k) => {
                    let ct = ComponentTransferFilter {
                        red_function: transfer(k.func_r()),
                        green_function: transfer(k.func_g()),
                        blue_function: transfer(k.func_b()),
                        alpha_function: transfer(k.func_a()),
                    };
                    let inputs = one(g, k.input(), &mut input);
                    g.add_svg(FilterEffect::ComponentTransfer(ct), inputs, Some(sub), linear)
                }
                uf::Kind::Composite(k) => {
                    let op = match k.operator() {
                        uf::CompositeOperator::Over => CompositeOperator::Over,
                        uf::CompositeOperator::In => CompositeOperator::In,
                        uf::CompositeOperator::Out => CompositeOperator::Out,
                        uf::CompositeOperator::Atop => CompositeOperator::Atop,
                        uf::CompositeOperator::Xor => CompositeOperator::Xor,
                        uf::CompositeOperator::Arithmetic { k1, k2, k3, k4 } => {
                            CompositeOperator::Arithmetic(ArithmeticCompositeOperator { k1, k2, k3, k4 })
                        }
                    };
                    let inputs = FilterInputs { primary: input(g, k.input1()), secondary: input(g, k.input2()) };
                    g.add_svg(FilterEffect::Composite(op), inputs, Some(sub), linear)
                }
                uf::Kind::ConvolveMatrix(k) => {
                    let m = k.matrix();
                    let inputs = one(g, k.input(), &mut input);
                    // anyrender's kernels are square with the target at the centre (SVG's default)
                    if m.columns() == m.rows() && m.target_x() == m.columns() / 2 && m.target_y() == m.rows() / 2 {
                        let kernel = ConvolutionKernel {
                            size: m.columns(),
                            values: m.data().to_vec(),
                            divisor: k.divisor().get(),
                            bias: k.bias(),
                            preserve_alpha: k.preserve_alpha(),
                        };
                        g.add_svg(FilterEffect::ConvolveMatrix(kernel), inputs, Some(sub), linear)
                    } else {
                        g.add_svg(FilterEffect::Offset(Vec2::ZERO), inputs, Some(sub), linear)
                    }
                }
                uf::Kind::DiffuseLighting(k) => {
                    let effect = FilterEffect::DiffuseLighting(DiffuseLightingFilter {
                        surface_scale: k.surface_scale(),
                        diffuse_constant: k.diffuse_constant(),
                        kernel_unit_length: 1.0,
                        light_source: light(k.light_source()),
                        lighting_color: color(k.lighting_color(), 1.0),
                    });
                    let inputs = one(g, k.input(), &mut input);
                    g.add_svg(effect, inputs, Some(sub), linear)
                }
                uf::Kind::SpecularLighting(k) => {
                    let effect = FilterEffect::SpecularLighting(SpecularLightingFilter {
                        surface_scale: k.surface_scale(),
                        specular_constant: k.specular_constant(),
                        specular_exponent: k.specular_exponent(),
                        kernel_unit_length: 1.0,
                        light_source: light(k.light_source()),
                        lighting_color: color(k.lighting_color(), 1.0),
                    });
                    let inputs = one(g, k.input(), &mut input);
                    g.add_svg(effect, inputs, Some(sub), linear)
                }
                uf::Kind::DisplacementMap(k) => {
                    let effect = FilterEffect::DisplacementMap(DisplacementMapFilter {
                        scale: k.scale(),
                        x_channel: channel(k.x_channel_selector()),
                        y_channel: channel(k.y_channel_selector()),
                    });
                    let inputs = FilterInputs { primary: input(g, k.input1()), secondary: input(g, k.input2()) };
                    g.add_svg(effect, inputs, Some(sub), linear)
                }
                uf::Kind::DropShadow(k) => {
                    let effect = FilterEffect::DropShadow(DropShadow {
                        dx: k.dx(),
                        dy: k.dy(),
                        std_deviation: k.std_dev_x().get(),
                        std_deviation_y: k.std_dev_y().get(),
                        color: color(k.color(), k.opacity().get()),
                        edge_mode: EdgeMode::None,
                    });
                    let inputs = one(g, k.input(), &mut input);
                    g.add_svg(effect, inputs, Some(sub), linear)
                }
                uf::Kind::Flood(k) => {
                    let effect = FilterEffect::Flood(color(k.color(), k.opacity().get()));
                    g.add_svg(effect, FilterInputs::NONE, Some(sub), linear)
                }
                uf::Kind::GaussianBlur(k) => {
                    let effect = FilterEffect::GaussianBlur(GaussianBlurFilter {
                        std_deviation: k.std_dev_x().get(),
                        std_deviation_y: k.std_dev_y().get(),
                        edge_mode: EdgeMode::None,
                    });
                    let inputs = one(g, k.input(), &mut input);
                    g.add_svg(effect, inputs, Some(sub), linear)
                }
                // feImage of an image file; a referenced element still renders transparent
                uf::Kind::Image(k) => {
                    let effect = image_effect(k.root(), to_rect(p.rect()))
                        .unwrap_or(FilterEffect::Flood(AlphaColor::TRANSPARENT));
                    g.add_svg(effect, FilterInputs::NONE, Some(sub), linear)
                }
                uf::Kind::Merge(k) => {
                    let mut acc: Option<FilterId> = None;
                    for i in k.inputs() {
                        let top = input(g, i);
                        acc = Some(match acc {
                            None => g.add_svg(
                                FilterEffect::Offset(Vec2::ZERO),
                                FilterInputs { primary: top, secondary: None },
                                Some(sub),
                                linear,
                            ),
                            Some(below) => g.add_svg(
                                FilterEffect::Composite(CompositeOperator::Over),
                                FilterInputs { primary: top, secondary: Some(FilterInput::Result(below)) },
                                Some(sub),
                                linear,
                            ),
                        });
                    }
                    match acc {
                        Some(id) => id,
                        None => g.add_svg(FilterEffect::Flood(AlphaColor::TRANSPARENT), FilterInputs::NONE, Some(sub), linear),
                    }
                }
                uf::Kind::Morphology(k) => {
                    let operator = match k.operator() {
                        uf::MorphologyOperator::Erode => MorphologyOperator::Erode,
                        uf::MorphologyOperator::Dilate => MorphologyOperator::Dilate,
                    };
                    let effect = FilterEffect::Morphology(MorphologyFilter {
                        operator,
                        radius: k.radius_x().get(),
                        radius_y: k.radius_y().get(),
                    });
                    let inputs = one(g, k.input(), &mut input);
                    g.add_svg(effect, inputs, Some(sub), linear)
                }
                uf::Kind::Offset(k) => {
                    let effect = FilterEffect::Offset(Vec2::new(k.dx() as f64, k.dy() as f64));
                    let inputs = one(g, k.input(), &mut input);
                    g.add_svg(effect, inputs, Some(sub), linear)
                }
                uf::Kind::Tile(k) => {
                    let inputs = one(g, k.input(), &mut input);
                    g.add_svg(FilterEffect::Tile, inputs, Some(sub), linear)
                }
                uf::Kind::Turbulence(k) => {
                    let effect = FilterEffect::Turbulence(TurbulenceFilter {
                        base_frequency: k.base_frequency_x().get(),
                        base_frequency_y: k.base_frequency_y().get(),
                        stitch_tiles: k.stitch_tiles(),
                        num_octaves: k.num_octaves(),
                        seed: k.seed() as u32,
                        turbulence_type: match k.kind() {
                            uf::TurbulenceKind::FractalNoise => TurbulenceType::FractalNoise,
                            uf::TurbulenceKind::Turbulence => TurbulenceType::Turbulence,
                        },
                    });
                    g.add_svg(effect, FilterInputs::NONE, Some(sub), linear)
                }
            };
            names.insert(p.result(), (id, sub));
            last = Some(id);
        }
        // A filter without primitives draws nothing (transparent black).
        let out = last.unwrap_or_else(|| {
            self.graph
                .add_svg(FilterEffect::Flood(AlphaColor::TRANSPARENT), FilterInputs::NONE, Some(region), false)
        });
        self.graph.set_result(out);
        self.source = Some(out);
        region
    }
}

/// An feImage of an image file: its pixels and where usvg placed it. usvg lays the image out in the
/// primitive subregion with its origin at (0, 0) (preserveAspectRatio applied), so the subregion's
/// origin moves it into user space.
#[cfg(feature = "image")]
fn image_effect(root: &usvg::Group, subregion: Rect) -> Option<FilterEffect> {
    let img = find_image(root)?;
    if matches!(img.kind(), usvg::ImageKind::SVG(_)) {
        return None;
    }
    let decoded = crate::util::decode_raw_raster_image(img.kind()).ok()?;
    let (width, height) = (decoded.width(), decoded.height());
    let b = img.abs_bounding_box();
    let rect = Rect::new(b.left() as f64, b.top() as f64, b.right() as f64, b.bottom() as f64) + subregion.origin().to_vec2();
    Some(FilterEffect::Image(ExternalImageSource {
        image_id: 0,
        transform: None,
        image: Some(peniko::ImageData {
            data: peniko::Blob::new(Arc::new(decoded.into_vec())),
            format: peniko::ImageFormat::Rgba8,
            alpha_type: peniko::ImageAlphaType::Alpha,
            width,
            height,
        }),
        rect: Some(rect),
    }))
}

#[cfg(not(feature = "image"))]
fn image_effect(_root: &usvg::Group, _subregion: Rect) -> Option<FilterEffect> {
    None
}

#[cfg(feature = "image")]
fn find_image(g: &usvg::Group) -> Option<&usvg::Image> {
    g.children().iter().find_map(|n| match n {
        usvg::Node::Image(i) => Some(&**i),
        usvg::Node::Group(g) => find_image(g),
        _ => None,
    })
}

fn to_rect(r: usvg::NonZeroRect) -> Rect {
    Rect::new(r.left() as f64, r.top() as f64, r.right() as f64, r.bottom() as f64)
}

fn color(c: usvg::Color, opacity: f32) -> AlphaColor<Srgb> {
    AlphaColor::from_rgba8(c.red, c.green, c.blue, (opacity.clamp(0.0, 1.0) * 255.0).round() as u8)
}

fn channel(c: uf::ColorChannel) -> ColorChannel {
    match c {
        uf::ColorChannel::R => ColorChannel::Red,
        uf::ColorChannel::G => ColorChannel::Green,
        uf::ColorChannel::B => ColorChannel::Blue,
        uf::ColorChannel::A => ColorChannel::Alpha,
    }
}

fn transfer(f: &uf::TransferFunction) -> TransferFunction {
    match f {
        uf::TransferFunction::Identity => TransferFunction::Identity,
        uf::TransferFunction::Table(v) => TransferFunction::Table(v.clone().into()),
        uf::TransferFunction::Discrete(v) => TransferFunction::Discrete(v.clone()),
        uf::TransferFunction::Linear { slope, intercept } => {
            TransferFunction::Linear(LinearTransferFunction { slope: *slope, intercept: *intercept })
        }
        uf::TransferFunction::Gamma { amplitude, exponent, offset } => TransferFunction::Gamma(GammaTransferFunction {
            amplitude: *amplitude,
            exponent: *exponent,
            offset: *offset,
        }),
    }
}

fn light(l: uf::LightSource) -> LightSource {
    match l {
        uf::LightSource::DistantLight(d) => LightSource::Distant(DistantLightSource { azimuth: d.azimuth, elevation: d.elevation }),
        uf::LightSource::PointLight(p) => LightSource::Point(PointLightSource { x: p.x, y: p.y, z: p.z }),
        uf::LightSource::SpotLight(s) => LightSource::Spot(SpotLightSource {
            x: s.x,
            y: s.y,
            z: s.z,
            points_at_x: s.points_at_x,
            points_at_y: s.points_at_y,
            points_at_z: s.points_at_z,
            specular_exponent: s.specular_exponent.get(),
            limiting_cone_angle: s.limiting_cone_angle,
        }),
    }
}

fn to_mix(mode: usvg::BlendMode) -> BlendMode {
    use peniko::Mix;
    match mode {
        usvg::BlendMode::Normal => Mix::Normal,
        usvg::BlendMode::Multiply => Mix::Multiply,
        usvg::BlendMode::Screen => Mix::Screen,
        usvg::BlendMode::Overlay => Mix::Overlay,
        usvg::BlendMode::Darken => Mix::Darken,
        usvg::BlendMode::Lighten => Mix::Lighten,
        usvg::BlendMode::ColorDodge => Mix::ColorDodge,
        usvg::BlendMode::ColorBurn => Mix::ColorBurn,
        usvg::BlendMode::HardLight => Mix::HardLight,
        usvg::BlendMode::SoftLight => Mix::SoftLight,
        usvg::BlendMode::Difference => Mix::Difference,
        usvg::BlendMode::Exclusion => Mix::Exclusion,
        usvg::BlendMode::Hue => Mix::Hue,
        usvg::BlendMode::Saturation => Mix::Saturation,
        usvg::BlendMode::Color => Mix::Color,
        usvg::BlendMode::Luminosity => Mix::Luminosity,
    }
}

/// Blink's FEColorMatrix SaturateMatrix.
fn saturate_matrix(s: f32) -> [f32; 20] {
    let mut m = [0.0; 20];
    m[0] = 0.213 + 0.787 * s;
    m[1] = 0.715 - 0.715 * s;
    m[2] = 0.072 - 0.072 * s;
    m[5] = 0.213 - 0.213 * s;
    m[6] = 0.715 + 0.285 * s;
    m[7] = 0.072 - 0.072 * s;
    m[10] = 0.213 - 0.213 * s;
    m[11] = 0.715 - 0.715 * s;
    m[12] = 0.072 + 0.928 * s;
    m[18] = 1.0;
    m
}

/// Blink's FEColorMatrix HueRotateMatrix.
fn hue_rotate_matrix(deg: f32) -> [f32; 20] {
    let (sin, cos) = deg.to_radians().sin_cos();
    let mut m = [0.0; 20];
    m[0] = 0.213 + cos * 0.787 - sin * 0.213;
    m[1] = 0.715 - cos * 0.715 - sin * 0.715;
    m[2] = 0.072 - cos * 0.072 + sin * 0.928;
    m[5] = 0.213 - cos * 0.213 + sin * 0.143;
    m[6] = 0.715 + cos * 0.285 + sin * 0.140;
    m[7] = 0.072 - cos * 0.072 - sin * 0.283;
    m[10] = 0.213 - cos * 0.213 - sin * 0.787;
    m[11] = 0.715 - cos * 0.715 + sin * 0.715;
    m[12] = 0.072 + cos * 0.928 + sin * 0.072;
    m[18] = 1.0;
    m
}
