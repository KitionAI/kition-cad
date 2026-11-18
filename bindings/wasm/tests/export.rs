//! The drawing exports as PDF, SVG and PNG (KHP A35, A41; Kition platform K21): model space framed
//! to its extents, the same file through the host's `document.export` (`KitDocument::export`) and
//! through `file.export`, the command `cad_export` runs, which answers the file as base64 `data`.
//! Each file is checked with its own reader: a one-page PDF whose drawing fills the page's printable
//! area, an SVG that parses and whose geometry fills its `viewBox`, and a PNG that decodes, is not
//! one colour, and whose drawing fills the image. Before, the kit exported nothing.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_cad_kit::CadDocument;
use kition_kit::KitDocument;
use serde_json::json;

/// A drawing away from its limits (0,0 to 420,297): a 400 × 300 frame at (1000, 500) with a circle
/// and a line of text in it. Framed to the limits as well, it would fill a third of the width.
fn drawing() -> CadDocument {
    let mut doc = CadDocument::create("Plan.dxf", 420, 297).unwrap();
    doc.execute("rectang", json!({ "p1": [1000, 500], "p2": [1400, 800] })).unwrap();
    doc.execute("circle", json!({ "center": [1200, 650], "radius": 100 })).unwrap();
    doc.execute("text", json!({ "at": [1020, 520], "height": 20, "text": "Two words" })).unwrap();
    let bytes = doc.save(Some("Plan.dxf")).unwrap();
    CadDocument::open("Plan.dxf", &bytes).unwrap()
}

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn decode(text: &str) -> Vec<u8> {
    let (mut out, mut buffer, mut bits) = (Vec::new(), 0u32, 0u32);
    for byte in text.trim_end_matches('=').bytes() {
        let value = BASE64.iter().position(|known| *known == byte).expect("base64");
        buffer = (buffer << 6) | value as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buffer >> bits) & 0xff) as u8);
        }
    }
    out
}

/// The export of `format` both ways, which must be the same file.
fn export(doc: &mut CadDocument, format: &str) -> Vec<u8> {
    let revision = doc.revision();
    let direct = doc.export(format, &json!({})).unwrap_or_else(|error| panic!("export {format}: {error}"));
    let answer = doc.execute("file.export", json!({ "format": format })).unwrap_or_else(|error| panic!("file.export {format}: {error}"));
    assert_eq!(answer["format"], format, "{answer}");
    assert_eq!(decode(answer["data"].as_str().unwrap()), direct, "file.export and document.export write the same {format}");
    assert_eq!(answer["bytes"], direct.len());
    assert_eq!((doc.revision(), doc.dirty()), (revision, false), "an export never edits the drawing");
    direct
}

/// The bounding box of points, `[min x, min y, max x, max y]`.
fn bounds(points: &[(f64, f64)]) -> [f64; 4] {
    points.iter().fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |b, &(x, y)| [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)])
}

/// Whether the content's box `inner` lies in `outer` and fills at least `share` of it on one axis.
fn fills(inner: [f64; 4], outer: [f64; 4], share: f64) -> bool {
    let slack = 1e-6 * (outer[2] - outer[0]).max(outer[3] - outer[1]);
    let inside = inner[0] >= outer[0] - slack && inner[1] >= outer[1] - slack && inner[2] <= outer[2] + slack && inner[3] <= outer[3] + slack;
    let wide = (inner[2] - inner[0]) / (outer[2] - outer[0]);
    let tall = (inner[3] - inner[1]) / (outer[3] - outer[1]);
    inside && wide.max(tall) >= share
}

#[test]
fn issue_export_the_kit_offers_pdf_svg_and_png() {
    assert_eq!(CadDocument::exports(), vec!["pdf", "svg", "png"]);
    let command = drawing().commands().into_iter().find(|command| command.id == "file.export").expect("file.export is a command");
    assert!(!command.mutates, "an export never edits the drawing");
    let mut doc = drawing();
    let error = doc.execute("file.export", json!({ "format": "dwg" })).unwrap_err();
    assert!(error.contains("pdf, svg, png"), "an unknown format names the ones there are: {error}");
    let error = doc.export("dxf", &json!({})).unwrap_err();
    assert!(error.contains("pdf, svg, png"), "{error}");
}

/// The numbers before each path operator (`m`, `l`, `c`) of a PDF content stream.
fn pdf_points(content: &str) -> Vec<(f64, f64)> {
    let mut stack: Vec<f64> = Vec::new();
    let mut points = Vec::new();
    for token in content.split_ascii_whitespace() {
        if let Ok(number) = token.parse::<f64>() {
            stack.push(number);
            continue;
        }
        if matches!(token, "m" | "l" | "c") {
            points.extend(stack.as_chunks::<2>().0.iter().map(|[x, y]| (*x, *y)));
        }
        stack.clear();
    }
    points
}

#[test]
fn issue_export_pdf_is_one_page_framed_to_the_extents() {
    let pdf = export(&mut drawing(), "pdf");
    assert!(pdf.starts_with(b"%PDF-1."), "a PDF header");
    let text = String::from_utf8_lossy(&pdf);
    assert!(text.trim_end().ends_with("%%EOF"), "a complete PDF");
    assert!(text.contains("/Type /Pages /Kids [3 0 R] /Count 1"), "one page");
    assert_eq!(text.matches("/Type /Page ").count(), 1, "one page object");
    let media = text.split("/MediaBox [").nth(1).and_then(|rest| rest.split(']').next()).unwrap();
    let media: Vec<f64> = media.split_whitespace().map(|n| n.parse().unwrap()).collect();
    assert_eq!(media.len(), 4);
    // The page's content stream, inflated.
    let start = pdf.windows(7).position(|w| w == b"stream\n").unwrap() + 7;
    let end = start + pdf[start..].windows(10).position(|w| w == b"\nendstream").unwrap();
    let content = miniz_oxide::inflate::decompress_to_vec_zlib(&pdf[start..end]).unwrap();
    let points = pdf_points(&String::from_utf8(content).unwrap());
    assert!(points.len() > 20, "the drawing is on the page: {} points", points.len());
    let page = [media[0], media[1], media[2], media[3]];
    assert!(fills(bounds(&points), page, 0.8), "the drawing fills the page: {:?} on {page:?}", bounds(&points));
}

/// A well-formed XML document: every tag closed in order, every attribute quoted. Answers the
/// elements as (name, attributes).
fn parse_xml(text: &str) -> Vec<(String, Vec<(String, String)>)> {
    let mut elements = Vec::new();
    let mut open: Vec<String> = Vec::new();
    let mut rest = text.trim();
    assert!(rest.starts_with('<'), "XML starts with a tag");
    while let Some(start) = rest.find('<') {
        assert!(rest[..start].trim().is_empty() || !open.is_empty(), "text outside the root element");
        let end = rest[start..].find('>').map(|end| start + end).expect("a tag ends");
        let tag = &rest[start + 1..end];
        rest = &rest[end + 1..];
        if let Some(name) = tag.strip_prefix('/') {
            assert_eq!(open.pop().as_deref(), Some(name.trim()), "tags close in order");
            continue;
        }
        let closed = tag.ends_with('/');
        let tag = tag.trim_end_matches('/');
        let name_end = tag.find(char::is_whitespace).unwrap_or(tag.len());
        let name = tag[..name_end].to_owned();
        assert!(!name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == ':' || c == '-'), "element name {name:?}");
        let mut attributes = Vec::new();
        let mut attrs = tag[name_end..].trim();
        while !attrs.is_empty() {
            let eq = attrs.find('=').unwrap_or_else(|| panic!("attribute without a value in <{name}>"));
            let key = attrs[..eq].trim().to_owned();
            let value = attrs[eq + 1..].strip_prefix('"').unwrap_or_else(|| panic!("unquoted attribute {key} in <{name}>"));
            let close = value.find('"').expect("a quoted value ends");
            assert!(!value[..close].contains('<'), "no `<` in a value");
            attributes.push((key, value[..close].to_owned()));
            attrs = value[close + 1..].trim();
        }
        if open.is_empty() {
            assert!(elements.is_empty(), "one root element");
        }
        elements.push((name.clone(), attributes));
        if !closed {
            open.push(name);
        }
    }
    assert!(open.is_empty() && rest.trim().is_empty(), "every element is closed: {open:?}");
    elements
}

#[test]
fn issue_export_svg_parses_and_is_framed_to_the_extents() {
    let svg = String::from_utf8(export(&mut drawing(), "svg")).unwrap();
    let elements = parse_xml(&svg);
    let attribute = |element: &(String, Vec<(String, String)>), key: &str| element.1.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
    let root = &elements[0];
    assert_eq!((root.0.as_str(), attribute(root, "xmlns").as_deref()), ("svg", Some("http://www.w3.org/2000/svg")));
    let view: Vec<f64> = attribute(root, "viewBox").unwrap().split_whitespace().map(|n| n.parse().unwrap()).collect();
    assert_eq!(view.len(), 4);
    let mut points = Vec::new();
    for element in elements.iter().filter(|element| element.0 == "polyline") {
        let numbers: Vec<f64> =
            attribute(element, "points").unwrap().split([' ', ',']).filter(|n| !n.is_empty()).map(|n| n.parse().unwrap()).collect();
        points.extend(numbers.as_chunks::<2>().0.iter().map(|[x, y]| (*x, *y)));
    }
    assert!(points.len() > 20, "the drawing is in the SVG");
    let frame = [view[0], view[1], view[0] + view[2], view[1] + view[3]];
    assert!(fills(bounds(&points), frame, 0.9), "the drawing fills the viewBox: {:?} in {frame:?}", bounds(&points));
}

#[test]
fn issue_export_png_decodes_is_not_uniform_and_is_framed_to_the_extents() {
    let png = export(&mut drawing(), "png");
    let image = image::load_from_memory_with_format(&png, image::ImageFormat::Png).unwrap().to_rgba8();
    let (width, height) = image.dimensions();
    assert!(width >= 1000 && height >= 1000, "{width} × {height}");
    let paper = *image.get_pixel(0, 0);
    let mut ink = Vec::new();
    for (x, y, pixel) in image.enumerate_pixels() {
        if *pixel != paper {
            ink.push((f64::from(x), f64::from(y)));
        }
    }
    assert!(ink.len() > 1000, "the PNG is not one colour: {} pixels differ from the paper", ink.len());
    let frame = [0.0, 0.0, f64::from(width), f64::from(height)];
    assert!(fills(bounds(&ink), frame, 0.8), "the drawing fills the image: {:?} in {width} × {height}", bounds(&ink));
}
