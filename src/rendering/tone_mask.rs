//! Rewrites an SVG into a mask document for gamut mapping.
//!
//! Every element inside a `data-byonk-tone="continuous"` subtree is painted
//! white; everything else is painted black. Rasterizing the result with the
//! same renderer, over a black background, yields a per-pixel mask saying
//! which pixels belong to a continuous-tone region.
//!
//! Recolouring rather than deleting is deliberate: it makes **occlusion just
//! work**, because an unmarked shape covering part of a marked photo correctly
//! masks it out — the renderer resolves z-order for us.
//!
//! # CSS
//!
//! A CSS rule beats a presentation attribute, and screen templates do set
//! `fill` from `<style>` blocks. Rather than depend on stylesheet precedence,
//! this rewriter **strips paint declarations from `<style>` content** and sets
//! paint on the elements. Geometry-affecting declarations are preserved,
//! because they change what area is covered.
//!
//! # Known over-marking
//!
//! Two cases grow the marked region slightly. Both are accepted:
//!
//! - An `<image>` becomes a `<rect>` over its layout box, so a transparent or
//!   letterboxed image marks its whole box. This applies to both the
//!   self-closing form and `<image>…</image>`, whose subtree is dropped.
//! - An element painted `none` only via CSS becomes painted here.
//!
//! Growing the region into unmarked territory is harmless — the mask
//! background is already black. Growing it inside a marked region maps a few
//! extra background pixels, and mapping in-gamut content is the identity.
//!
//! - A stroke set only by a stylesheet rule is lost, because paint
//!   declarations are stripped before the stroke is resolved. That element
//!   under-marks by half its stroke width. This is the one case that shrinks
//!   the region rather than growing it, and it is the fail-safe direction:
//!   the alternative, painting `stroke` unconditionally, invents a stroke on
//!   every unstroked shape and moves edges by an unbounded `stroke-width / 2`.

use quick_xml::events::attributes::Attribute;
use quick_xml::events::{BytesStart, BytesText, Event};
use quick_xml::{Reader, Writer};
use std::io::Cursor;

/// Marks an element and its descendants as continuous-tone.
pub const TONE_ATTR: &str = "data-byonk-tone";
/// Names the adaptation group an element belongs to.
pub const TONE_GROUP_ATTR: &str = "data-byonk-tone-group";

const WHITE: &str = "#ffffff";
const BLACK: &str = "#000000";

/// Paint properties whose value must come from us, not the document.
const PAINT_PROPS: [&str; 8] = [
    "fill",
    "stroke",
    "fill-opacity",
    "stroke-opacity",
    "opacity",
    "color",
    "stop-color",
    "stop-opacity",
];

#[derive(Debug, thiserror::Error)]
pub enum ToneMaskError {
    #[error("mask rewrite failed: {0}")]
    Xml(String),
}

/// Does this document mark anything at all?
///
/// Cheap enough to run on every render; when it returns false the caller skips
/// the mask rasterization entirely and the document renders exactly as it does
/// today.
pub fn has_tone_markup(svg: &[u8]) -> bool {
    svg.windows(TONE_ATTR.len() + 1).any(|w| {
        w[..TONE_ATTR.len()] == *TONE_ATTR.as_bytes()
            && (w[TONE_ATTR.len()] == b'=' || w[TONE_ATTR.len()] == b' ')
    })
}

/// Effective tone of an element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tone {
    Graphic,
    Continuous,
}

impl Tone {
    fn paint(self) -> &'static str {
        match self {
            Tone::Continuous => WHITE,
            Tone::Graphic => BLACK,
        }
    }
}

/// Whether an element paints a stroke, as far as the mask can tell.
///
/// The mask must not *introduce* a stroke where the document has none. SVG's
/// initial `stroke` is `none`, so painting `stroke` unconditionally would widen
/// every marked shape by half its stroke-width — and `stroke-width` survives
/// into the mask as geometry, so that error is unbounded, not sub-pixel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stroke {
    /// This element or an ancestor declares a stroke that is not `none`.
    Painted,
    /// No stroke declaration up the tree, or the nearest one is `none`.
    Absent,
}

/// Rewrite `svg` into its mask document.
pub fn build_mask_svg(svg: &[u8]) -> Result<Vec<u8>, ToneMaskError> {
    let mut reader = Reader::from_reader(svg);
    reader.config_mut().check_end_names = true;
    let mut writer = Writer::new(Cursor::new(Vec::new()));

    // Effective tone for each open element, innermost last.
    let mut tone_stack: Vec<Tone> = vec![Tone::Graphic];
    // Effective stroke for each open element, innermost last. The root default
    // is SVG's initial value, `none`.
    let mut stroke_stack: Vec<Stroke> = vec![Stroke::Absent];
    // Depth of open `<defs>` elements — content there is stripped, not painted.
    let mut defs_depth: usize = 0;
    // Depth of open `<style>` elements — text there is a stylesheet.
    let mut style_depth: usize = 0;
    // Depth inside a start-form `<image>` whose subtree we are swallowing.
    // `<image>…</image>` is legal and may hold `<title>`/`<desc>`; the element
    // is replaced by a rect, so nothing inside it may reach the mask.
    let mut image_skip_depth: usize = 0;
    let mut buf = Vec::new();

    loop {
        match reader
            .read_event_into(&mut buf)
            .map_err(|e| ToneMaskError::Xml(e.to_string()))?
        {
            Event::Eof => break,

            Event::Start(e) => {
                // Anything nested inside a replaced `<image>` is dropped.
                if image_skip_depth > 0 {
                    image_skip_depth += 1;
                    buf.clear();
                    continue;
                }
                let name = e.name().as_ref().to_vec();
                let tone = resolve_tone(&e, *tone_stack.last().unwrap());
                let stroke = resolve_stroke(&e, *stroke_stack.last().unwrap());
                if name == b"image" {
                    // Start-form image: emit the rect and swallow the subtree.
                    // No tone_stack push — the matching End is swallowed too.
                    let rect = image_to_rect(&e, tone, defs_depth > 0)?;
                    writer
                        .write_event(Event::Empty(rect))
                        .map_err(|e| ToneMaskError::Xml(e.to_string()))?;
                    image_skip_depth = 1;
                    buf.clear();
                    continue;
                }
                tone_stack.push(tone);
                stroke_stack.push(stroke);
                if name == b"defs" {
                    defs_depth += 1;
                }
                if name == b"style" {
                    style_depth += 1;
                }
                let out = rewrite_start(&e, tone, defs_depth > 0, stroke)?;
                writer
                    .write_event(Event::Start(out))
                    .map_err(|e| ToneMaskError::Xml(e.to_string()))?;
            }

            Event::End(e) => {
                if image_skip_depth > 0 {
                    image_skip_depth -= 1;
                    buf.clear();
                    continue;
                }
                let name = e.name().as_ref().to_vec();
                if name == b"defs" {
                    defs_depth = defs_depth.saturating_sub(1);
                }
                if name == b"style" {
                    style_depth = style_depth.saturating_sub(1);
                }
                if tone_stack.len() > 1 {
                    tone_stack.pop();
                }
                if stroke_stack.len() > 1 {
                    stroke_stack.pop();
                }
                writer
                    .write_event(Event::End(e))
                    .map_err(|e| ToneMaskError::Xml(e.to_string()))?;
            }

            Event::Empty(e) => {
                if image_skip_depth > 0 {
                    buf.clear();
                    continue;
                }
                // Self-closing: its tone applies to itself only, never to its
                // siblings.
                let tone = resolve_tone(&e, *tone_stack.last().unwrap());
                let stroke = resolve_stroke(&e, *stroke_stack.last().unwrap());
                if e.name().as_ref() == b"image" {
                    let rect = image_to_rect(&e, tone, defs_depth > 0)?;
                    writer
                        .write_event(Event::Empty(rect))
                        .map_err(|e| ToneMaskError::Xml(e.to_string()))?;
                } else {
                    let out = rewrite_start(&e, tone, defs_depth > 0, stroke)?;
                    writer
                        .write_event(Event::Empty(out))
                        .map_err(|e| ToneMaskError::Xml(e.to_string()))?;
                }
            }

            Event::Text(t) => {
                if image_skip_depth > 0 {
                    buf.clear();
                    continue;
                }
                if style_depth > 0 {
                    // `xml10_content` is quick-xml 0.41's unescaping accessor;
                    // the older `unescape()` no longer exists.
                    let css = t
                        .xml10_content()
                        .map_err(|e| ToneMaskError::Xml(e.to_string()))?;
                    let cleaned = strip_paint_declarations(&css);
                    writer
                        .write_event(Event::Text(BytesText::new(&cleaned)))
                        .map_err(|e| ToneMaskError::Xml(e.to_string()))?;
                } else {
                    writer
                        .write_event(Event::Text(t))
                        .map_err(|e| ToneMaskError::Xml(e.to_string()))?;
                }
            }

            Event::CData(c) if style_depth > 0 => {
                let css = String::from_utf8_lossy(c.as_ref()).to_string();
                let cleaned = strip_paint_declarations(&css);
                writer
                    .write_event(Event::Text(BytesText::new(&cleaned)))
                    .map_err(|e| ToneMaskError::Xml(e.to_string()))?;
            }

            other => {
                if image_skip_depth > 0 {
                    buf.clear();
                    continue;
                }
                writer
                    .write_event(other)
                    .map_err(|e| ToneMaskError::Xml(e.to_string()))?;
            }
        }
        buf.clear();
    }

    Ok(writer.into_inner().into_inner())
}

/// An element's effective stroke: its own declaration if it has one, else its
/// parent's.
///
/// Only presentation attributes and inline styles count as evidence. A stroke
/// set solely by a stylesheet rule cannot be seen here, because paint
/// declarations are stripped from stylesheets before this runs. Such an element
/// loses its stroke in the mask and therefore *under*-marks by half its stroke
/// width — the fail-safe direction, and the third known case in the module docs.
fn resolve_stroke(e: &BytesStart, inherited: Stroke) -> Stroke {
    let mut from_attr: Option<Stroke> = None;
    let mut from_style: Option<Stroke> = None;

    for attr in e.attributes().with_checks(false).flatten() {
        let key = String::from_utf8_lossy(attr.key.as_ref()).to_ascii_lowercase();
        let value = String::from_utf8_lossy(attr.value.as_ref()).into_owned();
        match key.as_str() {
            "stroke" => from_attr = Some(stroke_from_value(&value)),
            "style" => {
                from_style = declaration_value(&value, "stroke").map(|v| stroke_from_value(&v));
            }
            _ => {}
        }
    }

    // An inline style beats a presentation attribute, whatever the source order.
    from_style.or(from_attr).unwrap_or(inherited)
}

fn stroke_from_value(value: &str) -> Stroke {
    if value.trim().eq_ignore_ascii_case("none") {
        Stroke::Absent
    } else {
        Stroke::Painted
    }
}

/// The value of `prop` in an inline `style="..."`.
///
/// Uses the same property-name normalisation as [`is_paint_declaration`]:
/// names are case-insensitive and whitespace before the colon is legal. The
/// last declaration wins, as in CSS, and a trailing `!important` is dropped.
fn declaration_value(style: &str, prop: &str) -> Option<String> {
    let mut found = None;
    for decl in style.split(';') {
        let Some((name, value)) = decl.split_once(':') else {
            continue;
        };
        if name.trim().to_ascii_lowercase() == prop {
            let value = value.trim();
            let value = value
                .strip_suffix("!important")
                .map(str::trim_end)
                .unwrap_or(value);
            found = Some(value.trim().to_string());
        }
    }
    found
}

/// An element's effective tone: its own attribute if present, else its parent's.
fn resolve_tone(e: &BytesStart, inherited: Tone) -> Tone {
    for attr in e.attributes().with_checks(false).flatten() {
        if attr.key.as_ref() == TONE_ATTR.as_bytes() {
            return match attr.value.as_ref() {
                b"continuous" => Tone::Continuous,
                _ => Tone::Graphic,
            };
        }
    }
    inherited
}

/// Copy an element, replacing paint with the mask colour.
///
/// Inside `<defs>` paint is stripped instead, so a `<use>` site decides the
/// polarity of the content it pulls in.
fn rewrite_start(
    e: &BytesStart,
    tone: Tone,
    in_defs: bool,
    stroke_state: Stroke,
) -> Result<BytesStart<'static>, ToneMaskError> {
    let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
    let mut out = BytesStart::new(name);

    let mut fill_none_attr: Option<bool> = None;
    let mut fill_none_style: Option<bool> = None;
    let mut kept_style = String::new();

    for attr in e.attributes().with_checks(false) {
        let attr = attr.map_err(|e| ToneMaskError::Xml(e.to_string()))?;
        let key = String::from_utf8_lossy(attr.key.as_ref()).to_ascii_lowercase();
        let value = String::from_utf8_lossy(attr.value.as_ref()).into_owned();

        // The markers are ours; they must not survive into the mask document.
        if key == TONE_ATTR || key == TONE_GROUP_ATTR {
            continue;
        }
        // Paint we are about to set ourselves.
        if PAINT_PROPS.contains(&key.as_str()) {
            if key == "fill" {
                fill_none_attr = Some(value.trim().eq_ignore_ascii_case("none"));
            }
            continue;
        }
        if key == "style" {
            // Held back until the paint is known, so both land in one attribute.
            kept_style = strip_paint_declarations_inline(&value);
            // Same normalisation as the stylesheet stripper: `fill : NONE` is
            // a legal way to write it, and the old exact-match check missed it.
            fill_none_style =
                declaration_value(&value, "fill").map(|v| v.eq_ignore_ascii_case("none"));
            continue;
        }
        out.push_attribute(Attribute::from((key.as_str(), value.as_str())));
    }

    if !in_defs {
        let paint = tone.paint();
        // An inline style beats a presentation attribute.
        let fill_none = fill_none_style.or(fill_none_attr).unwrap_or(false);
        let fill = if fill_none { "none" } else { paint };
        // Never introduce a stroke the document does not have: SVG's initial
        // stroke is `none`, and `stroke-width` survives into the mask, so
        // painting unconditionally would move every edge by stroke-width/2.
        let stroke = match stroke_state {
            Stroke::Painted => paint,
            Stroke::Absent => "none",
        };
        out.push_attribute(Attribute::from(("fill", fill)));
        out.push_attribute(Attribute::from(("stroke", stroke)));
        out.push_attribute(Attribute::from(("fill-opacity", "1")));
        out.push_attribute(Attribute::from(("stroke-opacity", "1")));
        out.push_attribute(Attribute::from(("opacity", "1")));
        // Belt and braces: a stylesheet rule beats a presentation attribute, so
        // the paint goes in the inline style too. Stripping is the first line of
        // defence; this is what holds if a paint declaration ever survives it.
        push_style(&mut out, &kept_style, Some((fill, stroke)));
    } else {
        push_style(&mut out, &kept_style, None);
    }

    Ok(out)
}

/// Write the `style` attribute, merging the document's surviving declarations
/// with our paint. Omitted entirely when there is nothing to say.
fn push_style(out: &mut BytesStart<'static>, kept: &str, paint: Option<(&str, &str)>) {
    let mut style = String::new();
    let kept = kept.trim().trim_end_matches(';');
    if !kept.is_empty() {
        style.push_str(kept);
        style.push(';');
    }
    if let Some((fill, stroke)) = paint {
        style.push_str(&format!(
            "fill:{fill};stroke:{stroke};fill-opacity:1;stroke-opacity:1;opacity:1"
        ));
    }
    let style = style.trim_end_matches(';');
    if !style.is_empty() {
        out.push_attribute(Attribute::from(("style", style)));
    }
}

/// Replace an `<image>` with a solid rect over its layout box.
///
/// An image's pixels are not a paint, so it cannot be recoloured. The box is
/// the closest honest approximation; see the module docs on over-marking.
fn image_to_rect(
    e: &BytesStart,
    tone: Tone,
    in_defs: bool,
) -> Result<BytesStart<'static>, ToneMaskError> {
    let mut rect = BytesStart::new("rect");

    for attr in e.attributes().with_checks(false) {
        let attr = attr.map_err(|e| ToneMaskError::Xml(e.to_string()))?;
        let key = String::from_utf8_lossy(attr.key.as_ref()).into_owned();
        let value = String::from_utf8_lossy(attr.value.as_ref()).into_owned();
        // Geometry and placement carry over; the pixel source does not.
        if matches!(
            key.as_str(),
            "x" | "y" | "width" | "height" | "transform" | "clip-path" | "mask" | "id" | "class"
        ) {
            rect.push_attribute(Attribute::from((key.as_str(), value.as_str())));
        }
    }

    if !in_defs {
        let paint = tone.paint();
        rect.push_attribute(Attribute::from(("fill", paint)));
        rect.push_attribute(Attribute::from(("stroke", "none")));
        rect.push_attribute(Attribute::from(("fill-opacity", "1")));
        rect.push_attribute(Attribute::from(("opacity", "1")));
        push_style(&mut rect, "", Some((paint, "none")));
    }

    Ok(rect)
}

/// Remove paint declarations from a stylesheet, keeping geometry ones.
fn strip_paint_declarations(css: &str) -> String {
    // Operate declaration by declaration; braces and selectors pass through.
    let mut out = String::with_capacity(css.len());
    for chunk in css.split_inclusive([';', '{', '}']) {
        if is_paint_declaration(chunk) {
            continue;
        }
        out.push_str(chunk);
    }
    out
}

/// Same, for a `style="..."` attribute value (no selectors or braces).
fn strip_paint_declarations_inline(style: &str) -> String {
    style
        .split(';')
        .filter(|d| !is_paint_declaration(d))
        .map(|d| d.trim())
        .filter(|d| !d.is_empty())
        .collect::<Vec<_>>()
        .join(";")
}

/// Does this declaration set a paint property?
fn is_paint_declaration(decl: &str) -> bool {
    let body = decl.trim_end_matches([';', '{', '}']);
    let Some((prop, _)) = body.rsplit_once(':') else {
        return false;
    };
    // The property name is the last token before the colon — everything
    // before it is a selector or the tail of a previous declaration.
    // CSS property names are case-insensitive, and whitespace is legal before
    // the colon (`fill : red`). Both forms must be recognised: a paint
    // declaration that survives into the mask beats our presentation attribute
    // and silently inverts that element's mask polarity.
    let prop = prop
        .trim_end()
        .rsplit([' ', '\t', '\n', '{', '}', ';'])
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    PAINT_PROPS.contains(&prop.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mask_of(svg: &str) -> String {
        String::from_utf8(build_mask_svg(svg.as_bytes()).expect("rewrite must succeed")).unwrap()
    }

    #[test]
    fn presence_check_is_exact() {
        assert!(!has_tone_markup(br#"<svg><rect fill="red"/></svg>"#));
        assert!(has_tone_markup(
            br#"<svg><g data-byonk-tone="continuous"><rect/></g></svg>"#
        ));
        // A near-miss must not trigger the expensive path.
        assert!(!has_tone_markup(br#"<svg data-byonk-tone-ish="x"/></svg>"#));
    }

    #[test]
    fn unmarked_shapes_become_black() {
        let out = mask_of(
            r##"<svg xmlns="http://www.w3.org/2000/svg"><g data-byonk-tone="continuous"><rect id="a"/></g><rect id="b" fill="#ff0000"/></svg>"##,
        );
        let b = out.split(r#"id="b""#).nth(1).unwrap();
        assert!(b.contains("#000000"), "unmarked rect must be black: {b}");
        assert!(!b.contains("#ff0000"), "original paint must be gone: {b}");
    }

    #[test]
    fn marked_shapes_become_white() {
        let out = mask_of(
            r##"<svg xmlns="http://www.w3.org/2000/svg"><g data-byonk-tone="continuous"><rect id="a" fill="#123456"/></g></svg>"##,
        );
        let a = out.split(r#"id="a""#).nth(1).unwrap();
        assert!(a.contains("#ffffff"), "marked rect must be white: {a}");
    }

    #[test]
    fn marking_is_inherited_by_descendants() {
        let out = mask_of(
            r#"<svg xmlns="http://www.w3.org/2000/svg"><g data-byonk-tone="continuous"><g><circle id="deep"/></g></g></svg>"#,
        );
        let d = out.split(r#"id="deep""#).nth(1).unwrap();
        assert!(
            d.contains("#ffffff"),
            "descendant must inherit continuous: {d}"
        );
    }

    #[test]
    fn a_descendant_can_override_back_to_graphic() {
        let out = mask_of(
            r#"<svg xmlns="http://www.w3.org/2000/svg"><g data-byonk-tone="continuous"><rect id="bg"/><text id="label" data-byonk-tone="graphic">18:42</text></g></svg>"#,
        );
        let bg = out.split(r#"id="bg""#).nth(1).unwrap();
        let label = out.split(r#"id="label""#).nth(1).unwrap();
        assert!(bg.contains("#ffffff"), "background must be marked: {bg}");
        assert!(
            label.contains("#000000"),
            "override must unmark the label: {label}"
        );
    }

    #[test]
    fn tone_scope_closes_with_its_element() {
        let out = mask_of(
            r#"<svg xmlns="http://www.w3.org/2000/svg"><g data-byonk-tone="continuous"><rect id="in"/></g><rect id="out"/></svg>"#,
        );
        assert!(out.split(r#"id="in""#).nth(1).unwrap().contains("#ffffff"));
        assert!(out.split(r#"id="out""#).nth(1).unwrap().contains("#000000"));
    }

    #[test]
    fn self_closing_marked_element_does_not_leak_scope() {
        let out = mask_of(
            r#"<svg xmlns="http://www.w3.org/2000/svg"><rect id="m" data-byonk-tone="continuous"/><rect id="after"/></svg>"#,
        );
        assert!(out.split(r#"id="m""#).nth(1).unwrap().contains("#ffffff"));
        assert!(
            out.split(r#"id="after""#)
                .nth(1)
                .unwrap()
                .contains("#000000"),
            "a self-closing marked element must not mark its siblings"
        );
    }

    #[test]
    fn fill_none_is_preserved() {
        let out = mask_of(
            r##"<svg xmlns="http://www.w3.org/2000/svg"><g data-byonk-tone="continuous"><rect id="a" fill="none" stroke="#f00"/></g></svg>"##,
        );
        let a = out.split(r#"id="a""#).nth(1).unwrap();
        assert!(a.contains(r#"fill="none""#), "fill:none must survive: {a}");
        assert!(
            a.contains(r##"stroke="#ffffff""##),
            "stroke must be marked: {a}"
        );
    }

    #[test]
    fn css_paint_declarations_are_stripped_but_geometry_survives() {
        let out = mask_of(
            r#"<svg xmlns="http://www.w3.org/2000/svg"><defs><style>.date { font-size: 11px; fill: #555555; stroke: red; font-family: Outfit; }</style></defs><text class="date" id="t">x</text></svg>"#,
        );
        assert!(!out.contains("#555555"), "CSS fill must be stripped: {out}");
        assert!(
            !out.contains("stroke: red"),
            "CSS stroke must be stripped: {out}"
        );
        assert!(
            out.contains("font-size: 11px"),
            "geometry CSS must survive: {out}"
        );
        assert!(
            out.contains("font-family: Outfit"),
            "geometry CSS must survive: {out}"
        );
    }

    #[test]
    fn images_become_rects_over_their_box() {
        let out = mask_of(
            r#"<svg xmlns="http://www.w3.org/2000/svg"><g data-byonk-tone="continuous"><image id="p" x="10" y="20" width="100" height="50" href="p.png"/></g></svg>"#,
        );
        assert!(!out.contains("<image"), "image must be replaced: {out}");
        assert!(
            out.contains(r#"x="10""#) && out.contains(r#"width="100""#),
            "box must survive: {out}"
        );
        assert!(out.contains("#ffffff"), "image box must be marked: {out}");
    }

    #[test]
    fn defs_content_loses_paint_so_use_sites_decide() {
        let out = mask_of(
            r##"<svg xmlns="http://www.w3.org/2000/svg"><defs><g id="sym"><rect id="sr" fill="#abcdef"/></g></defs><use href="#sym" id="u"/></svg>"##,
        );
        // Scope to this element's own tag — the rest of the document legitimately
        // contains painted elements, and an unscoped tail would match them.
        let sr = out
            .split(r#"id="sr""#)
            .nth(1)
            .unwrap()
            .split('>')
            .next()
            .unwrap();
        assert!(!sr.contains("#abcdef"), "defs paint must be stripped: {sr}");
        assert!(
            !sr.contains(r##"fill="#"##),
            "defs must not gain paint either: {sr}"
        );
        assert!(out.split(r#"id="u""#).nth(1).unwrap().contains("#000000"));
    }

    #[test]
    fn start_form_image_is_replaced_and_its_subtree_dropped() {
        // `<image>…</image>` is legal SVG (it may carry `<title>`/`<desc>`).
        // It must be replaced exactly like the self-closing form; leaving it
        // intact would put the real photograph into the mask document, where
        // its pixels would threshold into an arbitrary mask.
        let out = mask_of(
            r##"<svg xmlns="http://www.w3.org/2000/svg"><g data-byonk-tone="continuous"><image id="p" x="10" y="20" width="100" height="50" href="p.png"><title>a caption</title></image></g><rect id="after"/></svg>"##,
        );
        assert!(
            !out.contains("<image"),
            "start-form image must be replaced: {out}"
        );
        assert!(!out.contains("</image>"), "no orphan end tag: {out}");
        assert!(
            !out.contains("a caption"),
            "image subtree must be dropped: {out}"
        );
        let p = out.split(r#"id="p""#).nth(1).unwrap();
        assert!(p.contains(r#"width="100""#), "box must survive: {p}");
        assert!(p.contains("#ffffff"), "image box must be marked: {p}");
        assert!(
            out.split(r#"id="after""#)
                .nth(1)
                .unwrap()
                .contains("#000000"),
            "swallowing the subtree must not disturb later siblings: {out}"
        );
    }

    #[test]
    fn malformed_xml_is_an_error_not_a_silent_fallback() {
        assert!(build_mask_svg(b"<svg><g></svg>").is_err());
    }

    #[test]
    fn tone_attributes_are_dropped_from_the_mask_document() {
        let out = mask_of(
            r#"<svg xmlns="http://www.w3.org/2000/svg"><g data-byonk-tone="continuous" data-byonk-tone-group="sky"><rect/></g></svg>"#,
        );
        assert!(
            !out.contains("data-byonk-tone"),
            "marker must not survive: {out}"
        );
    }

    #[test]
    fn css_paint_is_stripped_case_insensitively_and_around_whitespace() {
        // CSS property names are case-insensitive and allow whitespace before
        // the colon. A paint declaration that survives into the mask beats the
        // presentation attribute and silently inverts that element's polarity.
        for decl in [
            "FILL: red;",
            "Fill: red;",
            "fill : red;",
            "STROKE: red;",
            "fill\t: red;",
        ] {
            let svg = format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg"><style>.d {{ {decl} }}</style><rect class="d" id="r"/></svg>"#
            );
            let out = mask_of(&svg);
            assert!(!out.contains("red"), "{decl} must be stripped: {out}");
        }
    }

    #[test]
    fn paint_is_written_to_the_inline_style_as_well() {
        // A stylesheet rule beats a presentation attribute, so the paint must
        // also be in the inline style, which beats both.
        let out = mask_of(
            r##"<svg xmlns="http://www.w3.org/2000/svg"><g data-byonk-tone="continuous"><rect id="a"/><rect id="s" stroke="#123456"/></g><rect id="b"/></svg>"##,
        );
        let a = out
            .split(r#"id="a""#)
            .nth(1)
            .unwrap()
            .split('>')
            .next()
            .unwrap();
        assert!(
            a.contains("style="),
            "marked element needs an inline style: {a}"
        );
        assert!(
            a.contains("fill:#ffffff"),
            "inline style must carry paint: {a}"
        );
        // `#a` declares no stroke, and SVG's initial stroke is `none`. The mask
        // must not invent one: a stroke here would widen the marked area by
        // half the stroke-width, which `stroke-width` in the document can make
        // arbitrarily large.
        assert!(
            a.contains("stroke:none"),
            "an unstroked element must not gain a stroke: {a}"
        );
        // A genuinely stroked element keeps its stroke, repainted.
        let s = out
            .split(r#"id="s""#)
            .nth(1)
            .unwrap()
            .split('>')
            .next()
            .unwrap();
        assert!(
            s.contains("stroke:#ffffff"),
            "a stroked element must keep its stroke, repainted: {s}"
        );
        let b = out
            .split(r#"id="b""#)
            .nth(1)
            .unwrap()
            .split('>')
            .next()
            .unwrap();
        assert!(
            b.contains("fill:#000000"),
            "unmarked element inline style: {b}"
        );
    }

    #[test]
    fn inline_style_keeps_geometry_and_replaces_only_paint() {
        let out = mask_of(
            r##"<svg xmlns="http://www.w3.org/2000/svg"><g data-byonk-tone="continuous"><text id="t" style="font-size:11px;fill:#555555">x</text></g></svg>"##,
        );
        let t = out
            .split(r#"id="t""#)
            .nth(1)
            .unwrap()
            .split('>')
            .next()
            .unwrap();
        assert!(t.contains("font-size:11px"), "geometry must survive: {t}");
        assert!(!t.contains("#555555"), "original paint must be gone: {t}");
        assert!(t.contains("fill:#ffffff"), "our paint must be present: {t}");
    }

    #[test]
    fn defs_content_gets_no_inline_paint() {
        let out = mask_of(
            r##"<svg xmlns="http://www.w3.org/2000/svg"><defs><rect id="sr" style="font-size:9px;fill:#abcdef"/></defs></svg>"##,
        );
        let sr = out
            .split(r#"id="sr""#)
            .nth(1)
            .unwrap()
            .split('>')
            .next()
            .unwrap();
        assert!(
            sr.contains("font-size:9px"),
            "geometry must survive in defs: {sr}"
        );
        assert!(!sr.contains("fill:"), "defs must gain no paint: {sr}");
    }

    #[test]
    fn an_unstroked_shape_gains_no_stroke_in_the_mask() {
        // The defect this guards: SVG's initial stroke is `none`, so painting
        // `stroke` unconditionally invented a stroke-width/2 border on every
        // marked shape. Measured before the fix: a shape spanning x=50..=149
        // rasterized to 50..=150, and to 40..=159 with a stroke-width of 20.
        let out = mask_of(
            r##"<svg xmlns="http://www.w3.org/2000/svg"><g data-byonk-tone="continuous"><rect id="r" width="10" height="10"/></g></svg>"##,
        );
        let r = out
            .split(r#"id="r""#)
            .nth(1)
            .unwrap()
            .split('>')
            .next()
            .unwrap();
        assert!(r.contains(r#"stroke="none""#), "no invented stroke: {r}");
        assert!(
            !r.contains(r##"stroke="#ffffff""##),
            "no invented stroke: {r}"
        );
    }

    #[test]
    fn a_stroked_shape_keeps_its_stroke_repainted() {
        let out = mask_of(
            r##"<svg xmlns="http://www.w3.org/2000/svg"><g data-byonk-tone="continuous"><path id="p" fill="none" stroke="#123456" d="M0 0 L10 10"/></g></svg>"##,
        );
        let p = out
            .split(r#"id="p""#)
            .nth(1)
            .unwrap()
            .split('>')
            .next()
            .unwrap();
        // A stroke-only shape has no fill area at all — dropping its stroke
        // would erase it from the mask entirely.
        assert!(
            p.contains(r##"stroke="#ffffff""##),
            "stroke kept and repainted: {p}"
        );
        assert!(p.contains(r#"fill="none""#), "fill:none preserved: {p}");
    }

    #[test]
    fn stroke_is_inherited_from_an_ancestor_group() {
        // `<g stroke="..."><line/></g>` is legal and common: the line has no
        // stroke of its own but inherits one. Writing stroke="none" on it
        // would override the inherited paint and erase the line.
        let out = mask_of(
            r##"<svg xmlns="http://www.w3.org/2000/svg"><g data-byonk-tone="continuous" stroke="#000000"><line id="l" x1="0" y1="0" x2="10" y2="10"/></g></svg>"##,
        );
        let l = out
            .split(r#"id="l""#)
            .nth(1)
            .unwrap()
            .split('>')
            .next()
            .unwrap();
        assert!(
            l.contains(r##"stroke="#ffffff""##),
            "inherited stroke kept: {l}"
        );
    }

    #[test]
    fn an_explicit_stroke_none_still_suppresses_the_stroke() {
        let out = mask_of(
            r##"<svg xmlns="http://www.w3.org/2000/svg"><g data-byonk-tone="continuous" stroke="#000000"><rect id="r" stroke="none" width="10" height="10"/></g></svg>"##,
        );
        let r = out
            .split(r#"id="r""#)
            .nth(1)
            .unwrap()
            .split('>')
            .next()
            .unwrap();
        assert!(
            r.contains(r#"stroke="none""#),
            "explicit none wins over inherited: {r}"
        );
    }

    #[test]
    fn inline_style_stroke_is_recognised_around_case_and_whitespace() {
        // Same normalisation the stylesheet stripper uses. An unrecognised
        // declaration here would drop a real stroke from the mask.
        for decl in [
            "stroke:#123456",
            "STROKE: #123456",
            "Stroke : #123456",
            "stroke\t: #123456",
            "stroke: #123456 !important",
        ] {
            let svg = format!(
                r##"<svg xmlns="http://www.w3.org/2000/svg"><g data-byonk-tone="continuous"><rect id="r" style="{decl}" width="10" height="10"/></g></svg>"##
            );
            let out = mask_of(&svg);
            let r = out
                .split(r#"id="r""#)
                .nth(1)
                .unwrap()
                .split('>')
                .next()
                .unwrap();
            assert!(
                r.contains(r##"stroke="#ffffff""##),
                "{decl} must be seen as a stroke: {r}"
            );
        }
    }

    #[test]
    fn an_inline_style_stroke_beats_the_presentation_attribute() {
        // CSS cascade: the inline style wins whatever the attribute order.
        let out = mask_of(
            r##"<svg xmlns="http://www.w3.org/2000/svg"><g data-byonk-tone="continuous"><rect id="r" stroke="none" style="stroke:#123456" width="10" height="10"/><rect id="q" stroke="#123456" style="stroke:none" width="10" height="10"/></g></svg>"##,
        );
        let r = out
            .split(r#"id="r""#)
            .nth(1)
            .unwrap()
            .split('>')
            .next()
            .unwrap();
        assert!(
            r.contains(r##"stroke="#ffffff""##),
            "style beats attribute: {r}"
        );
        let q = out
            .split(r#"id="q""#)
            .nth(1)
            .unwrap()
            .split('>')
            .next()
            .unwrap();
        assert!(
            q.contains(r#"stroke="none""#),
            "style:none beats attribute: {q}"
        );
    }
}
