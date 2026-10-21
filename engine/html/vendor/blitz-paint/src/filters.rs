use blitz_dom::util::ToColorColor as _;
use style::color::AbsoluteColor;
pub(crate) use style::computed_values::filter::single_value::T as StyloFilter;

use anyrender::filters::{Filter, FilterEffect};
#[cfg(feature = "svg")]
use anyrender::filters::{FilterId, FilterInput, FilterInputs, FilterSource};

// raea: SVG `<filter>`s convert with anyrender_svg::filter (also used for filters inside an SVG)
#[cfg(feature = "svg")]
use anyrender_svg::filter as svg_filter;

pub(crate) fn convert_filters(filters: &[StyloFilter]) -> Option<Filter> {
    if filters.is_empty() {
        return None;
    }

    Some(Filter::linear_list(
        filters.iter().filter_map(convert_single_filter),
    ))
}

pub(crate) fn convert_single_filter(filter: &StyloFilter) -> Option<FilterEffect> {
    Some(match filter {
        StyloFilter::Blur(radius) => FilterEffect::blur(radius.px()),
        StyloFilter::Brightness(amount) => FilterEffect::brightness(amount.0),
        StyloFilter::Contrast(amount) => FilterEffect::contrast(amount.0),
        StyloFilter::Grayscale(amount) => FilterEffect::grayscale(amount.0),
        StyloFilter::HueRotate(angle) => FilterEffect::hue_rotate(angle.radians()),
        StyloFilter::Invert(amount) => FilterEffect::invert(amount.0),
        StyloFilter::Opacity(amount) => FilterEffect::opacity(amount.0),
        StyloFilter::Saturate(amount) => FilterEffect::saturate(amount.0),
        StyloFilter::Sepia(amount) => FilterEffect::sepia(amount.0),
        StyloFilter::DropShadow(shadow) => FilterEffect::drop_shadow(
            shadow.horizontal.px(),
            shadow.vertical.px(),
            shadow.blur.px(),
            // TODO: pass in correct currentColor
            shadow
                .color
                .resolve_to_absolute(&AbsoluteColor::BLACK)
                .as_color_color(),
        ),
        StyloFilter::Url(_) => return None,
    })
}

/// raea: an element's whole `filter` list as one graph, `url(#id)` items resolved to the SVG
/// `<filter>` they name the way Blink's FilterEffectBuilder builds reference filters: the reference box
/// is the border box, in CSS px with its top-left at the origin (`box_size`), and the graph's regions are
/// in that space. A reference that doesn't resolve drops the whole list (Chrome then paints the element
/// unfiltered).
pub(crate) fn convert_element_filters(
    dom: &blitz_dom::BaseDocument,
    filters: &[StyloFilter],
    box_size: (f64, f64),
) -> Option<Filter> {
    if !filters.iter().any(|f| matches!(f, StyloFilter::Url(_))) {
        return convert_filters(filters);
    }
    #[cfg(feature = "svg")]
    {
        let mut graph = Filter::empty();
        let mut prev: Option<FilterId> = None;
        let mut region: Option<kurbo::Rect> = None;
        for f in filters {
            if let StyloFilter::Url(url) = f {
                let svg = reference_filter(dom, url_fragment(url)?, box_size)?;
                let offset = graph.nodes().len() as u16;
                let source = prev;
                let map = |i: &Option<FilterInput>| {
                    i.as_ref().map(|i| match i {
                        FilterInput::Source(FilterSource::SourceGraphic) => source
                            .map(FilterInput::Result)
                            .unwrap_or(FilterInput::Source(FilterSource::SourceGraphic)),
                        FilterInput::Source(s) => FilterInput::Source(*s),
                        FilterInput::Result(id) => FilterInput::Result(FilterId(id.0 + offset)),
                    })
                };
                for node in svg.nodes() {
                    let inputs = FilterInputs { primary: map(&node.inputs.primary), secondary: map(&node.inputs.secondary) };
                    graph.add_svg(node.effect.clone(), inputs, node.subregion, node.linear_rgb);
                }
                prev = Some(FilterId(svg.output().0 + offset));
                let r = svg.region()?;
                region = Some(region.map_or(r, |a| a.union(r)));
            } else {
                let effect = convert_single_filter(f)?;
                let inputs = FilterInputs { primary: prev.map(FilterInput::Result), secondary: None };
                prev = Some(graph.add(effect, inputs));
            }
        }
        graph.set_result(prev?);
        graph.set_region(region);
        Some(graph)
    }
    #[cfg(not(feature = "svg"))]
    {
        let _ = (dom, box_size);
        convert_filters(filters)
    }
}

#[cfg(feature = "svg")]
thread_local! {
    /// Parsed reference filters by (markup, reference box): GSAP animating a primitive's attribute
    /// changes the markup, so a stale entry is never reused.
    static REFERENCE_FILTERS: std::cell::RefCell<std::collections::HashMap<(String, u64, u64), Option<Filter>>> =
        Default::default();
}

/// The `<filter>` that `url(#id)` names, as a graph for a reference box of `w` x `h` CSS px: usvg
/// resolves its units, defaults and inheritance against a rect of that size.
#[cfg(feature = "svg")]
fn reference_filter(dom: &blitz_dom::BaseDocument, id: String, (w, h): (f64, f64)) -> Option<Filter> {
    if !(w > 0.0 && h > 0.0) {
        return None;
    }
    let node = dom.get_node(dom.get_element_by_id(&id)?)?;
    if &*node.element_data()?.name.local != "filter" {
        return None;
    }
    // color-interpolation-filters inherits: the nearest ancestor's value goes on the wrapper <svg>
    let mut inherited = String::new();
    let mut parent = node.parent;
    while let Some(p) = parent.and_then(|p| dom.get_node(p)) {
        if let Some(v) = p.element_data().and_then(|e| {
            e.attrs().iter().find(|a| &*a.name.local == "color-interpolation-filters").map(|a| a.value.clone())
        }) {
            inherited = format!(" color-interpolation-filters=\"{v}\"");
            break;
        }
        parent = p.parent;
    }
    let fragment = node.svg_source();
    let key = (format!("{fragment}{inherited}"), w.to_bits(), h.to_bits());
    if let Some(hit) = REFERENCE_FILTERS.with(|c| c.borrow().get(&key).cloned()) {
        return hit;
    }
    let src = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" width=\"{w}\" height=\"{h}\"{inherited}>{}<rect width=\"{w}\" height=\"{h}\" filter=\"url(#{id})\"/></svg>",
        fragment
    );
    // relative hrefs (feImage of a track frame) resolve against the document's folder
    let options = usvg::Options {
        resources_dir: dom.url().to_file_path().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())),
        ..usvg::Options::default()
    };
    let graph = usvg::Tree::from_str(&src, &options)
        .ok()
        .and_then(|tree| group_filters(tree.root()).and_then(svg_filter::convert_filters));
    REFERENCE_FILTERS.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() > 256 {
            c.clear();
        }
        c.insert(key, graph.clone());
    });
    graph
}

/// The `#id` a filter's `url()` names. Servo's stylo from crates.io types filter's url() as
/// `Impossible` (it never parses `filter: url(...)`); framework's vendor/stylo keeps it as a
/// `ComputedUrl`. Both implement this, so blitz-paint builds against either.
#[cfg(feature = "svg")]
trait FilterUrl {
    fn fragment(&self) -> Option<String>;
}

#[cfg(feature = "svg")]
impl FilterUrl for style::values::Impossible {
    fn fragment(&self) -> Option<String> {
        match *self {}
    }
}

#[cfg(feature = "svg")]
impl FilterUrl for style::url::ComputedUrl {
    fn fragment(&self) -> Option<String> {
        match self {
            style::url::ComputedUrl::Valid(u) => u.fragment().map(str::to_owned),
            style::url::ComputedUrl::Invalid(s) => s.strip_prefix('#').map(str::to_owned),
        }
    }
}

#[cfg(feature = "svg")]
fn url_fragment<U: FilterUrl>(url: &U) -> Option<String> {
    url.fragment()
}

#[cfg(feature = "svg")]
fn group_filters(g: &usvg::Group) -> Option<&[std::sync::Arc<usvg::filter::Filter>]> {
    if !g.filters().is_empty() {
        return Some(g.filters());
    }
    g.children().iter().find_map(|n| match n {
        usvg::Node::Group(g) => group_filters(g),
        _ => None,
    })
}
