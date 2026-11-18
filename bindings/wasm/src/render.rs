//! `file.render`: a preview by explicit view (`kit.json` `agent.render`; Kit Process Protocol A24,
//! A39; task card C4/C5/C6-cad). A pure function of the drawing and the call: it never reads or
//! changes the space shown, the view, the zoom or the selection, and the same call answers the same
//! bytes.
//!
//! The view (`target`): a space by `layout` (a layout's name, or `Model`; Model space when left
//! out), and in it one of a `window` (`[x1, y1, x2, y2]` in drawing units), an entity by
//! `objectId` (its handle: framed to it with a margin, in its own space), a named `view` saved in
//! the drawing (Model space), or a diagnostic's `path` (`entity:<handle>`). Without one, the
//! space's extents with a margin (a layout's sheet and extents). Layers are drawn as the drawing
//! says (off and frozen layers stay hidden, as the person left them in the file); `layers` draws
//! only the layers it names.
//!
//! The answer's `window` is the rectangle drawn, in drawing units, and `unitsPerPixel` how many
//! drawing units one pixel covers: pixel column `px` (from the left) is `x = window[0] + px *
//! unitsPerPixel`, row `py` (from the top) `y = window[3] - py * unitsPerPixel`.

use kition_cad_doc::{Drawing, Handle, Space};
use kition_cad_geom::{Bounds2, Vec2};
use kition_cad_render::raster::{self as cad_raster, RasterOptions};
use kition_kit::KitCommand;
use serde_json::{Value, json};

use crate::{CadDocument, PAPER, context};

pub const RENDER: &str = "file.render";
/// Pixels on the longer side when the call gives no `scale`.
const DEFAULT_SIZE: u32 = 1024;
/// The most pixels on a side.
const MAX_SIZE: u32 = 4096;
/// The margin around extents and an object, as a fraction of its larger side.
const MARGIN: f64 = 0.05;
/// The members a `target` takes.
const MEMBERS: [&str; 5] = ["layout", "window", "objectId", "view", "path"];

/// The command, as the catalog lists it.
pub fn command() -> KitCommand {
    KitCommand {
        id: RENDER.to_owned(),
        title: "Render preview".to_owned(),
        description: "Draws a view of the drawing as a PNG, a function of the drawing and the call alone: a space by `layout` (Model when left out) fitted to its extents, or in it a `window` in drawing units, an entity framed by its handle (`objectId`) or a named `view`; `layers` draws only those layers. Never the space shown, the view or the selection, and never a change. Its `window` and `unitsPerPixel` turn a pixel into drawing coordinates.".to_owned(),
        params: Some("{target? {layout? (a layout's name, or \"Model\"), window? [x1, y1, x2, y2] (drawing units), objectId? (a handle), view? (a named view), path? (\"entity:<handle>\")}, layers? [names], scale? (pixels per drawing unit), maxSize? (pixels on the longer side, 1 to 4096, default 1024)} → {png (base64), mimeType, width, height, scale, layout, window, unitsPerPixel, drew}".to_owned()),
        schema: Some(json!({
            "target": {
                "type": "object",
                "additionalProperties": false,
                "description": "The view: a space (`layout`), and in it a `window`, an entity (`objectId`, or a diagnostic's `path` `entity:<handle>`) or a named `view`. Without one, Model space fitted to its extents.",
                "properties": {
                    "layout": { "type": "string", "description": "A layout's name as inspect lists it in `layouts`, or \"Model\" (the default)." },
                    "window": { "type": "array", "items": { "type": "number" }, "description": "[x1, y1, x2, y2]: the rectangle to draw, in drawing units of the space." },
                    "objectId": { "type": "string", "description": "An entity's handle: that entity framed with a margin, in its space." },
                    "view": { "type": "string", "description": "A named view saved in the drawing (inspect's `views`), in Model space." },
                    "path": { "type": "string", "description": "A diagnostic's target, `entity:<handle>`." },
                },
            },
            "layers": { "type": "array", "items": { "type": "string" }, "description": "Draw only these layers (by name); others are left out. Without it, the layers the drawing shows." },
            "scale": { "type": "number", "description": "Pixels per drawing unit, above 0; default: the view fitted to `maxSize`. A view larger than `maxSize` at that scale is drawn smaller." },
            "maxSize": { "type": "integer", "description": "The most pixels on the longer side, 1 to 4096; default 1024." },
        })),
        output_schema: Some(json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "png": { "type": "string" },
                "mimeType": { "type": "string" },
                "width": { "type": "integer" },
                "height": { "type": "integer" },
                "scale": { "type": "number" },
                "layout": { "type": "string" },
                "window": { "type": "array", "items": { "type": "number" } },
                "unitsPerPixel": { "type": "number" },
                "drew": { "type": "string" },
                "objectId": { "type": "string" },
                "view": { "type": "string" },
            },
        })),
        mutates: false,
        enabled: true,
        ..KitCommand::default()
    }
}

fn invalid(message: impl std::fmt::Display) -> String {
    format!("invalid-params: {message}")
}

fn not_found(message: impl std::fmt::Display) -> String {
    format!("not-found: {message}")
}

/// `bounds` with a margin of [`MARGIN`] of its larger side (at least 1 unit for a point).
fn with_margin(bounds: Bounds2) -> Bounds2 {
    let side = bounds.width().max(bounds.height());
    let m = if side > 0.0 { side * MARGIN } else { 1.0 };
    Bounds2::new(Vec2::new(bounds.min.x - m, bounds.min.y - m), Vec2::new(bounds.max.x + m, bounds.max.y + m))
}

/// What a call draws: the space, the rectangle, how it was chosen, and the object or view named.
struct Subject {
    space: Space,
    window: Bounds2,
    drew: &'static str,
    object: Option<String>,
    view: Option<String>,
}

fn entity_subject(d: &Drawing, text: &str, space: Option<&Space>) -> Result<Subject, String> {
    let handle =
        Handle::parse_hex(text).ok_or_else(|| invalid(format!("`objectId` is {text:?}, not an entity handle (a hex string such as \"1A2\")")))?;
    let (entity, own) = match (d.entity(handle), d.space_of(handle)) {
        (Some(entity), Some(own)) => (entity, own),
        _ => return Err(not_found(format!("the drawing has no entity {} in model space or a layout", handle.hex()))),
    };
    if let Some(space) = space
        && *space != own
    {
        return Err(invalid(format!(
            "entity {} is in {}, and the view names {}: name one space, or leave `layout` out (a handle names its space)",
            handle.hex(),
            context::space_name(&own),
            context::space_name(space)
        )));
    }
    let bounds = kition_cad_doc::entity_bounds(d, entity, 0);
    if bounds.is_empty() {
        return Err(invalid(format!("entity {} draws nothing", handle.hex())));
    }
    Ok(Subject { space: own, window: with_margin(bounds), drew: "object", object: Some(handle.hex()), view: None })
}

fn window_of(value: &Value) -> Result<Bounds2, String> {
    let numbers: Option<Vec<f64>> = value.as_array().filter(|a| a.len() == 4).and_then(|a| a.iter().map(Value::as_f64).collect());
    let [x1, y1, x2, y2] = numbers
        .as_deref()
        .and_then(|n| <[f64; 4]>::try_from(n).ok())
        .ok_or_else(|| invalid(format!("`target.window` is {value}: a window is [x1, y1, x2, y2], two corners in drawing units")))?;
    let window = Bounds2::new(Vec2::new(x1.min(x2), y1.min(y2)), Vec2::new(x1.max(x2), y1.max(y2)));
    kition_cad_io::check_window(&window).map_err(|e| invalid(format!("`target.window` {value}: {e}")))?;
    Ok(window)
}

/// The view `params` names ([`Subject`]).
fn subject(d: &Drawing, params: &Value) -> Result<Subject, String> {
    let target = match params.get("target") {
        None | Some(Value::Null) => serde_json::Map::new(),
        Some(Value::Object(members)) => members.clone(),
        Some(other) => return Err(invalid(format!("`target` must be an object, not {other}"))),
    };
    if let Some(other) = target.keys().find(|key| !MEMBERS.contains(&key.as_str())) {
        return Err(invalid(format!(
            "`target.{other}` is no view of a drawing: a view takes `layout` (a space), and in it `window`, `objectId`, `view` or `path`"
        )));
    }
    let space = match target.get("layout") {
        None | Some(Value::Null) => None,
        Some(Value::String(name)) => Some(context::space_named(d, name).map_err(not_found)?),
        Some(other) => return Err(invalid(format!("`target.layout` is {other}: a layout's name, or \"Model\""))),
    };
    let chosen: Vec<&str> = ["window", "objectId", "view", "path"].into_iter().filter(|key| target.get(*key).is_some_and(|v| !v.is_null())).collect();
    if chosen.len() > 1 {
        return Err(invalid(format!("the view names {}: give one of `window`, `objectId`, `view` or `path`", chosen.join(" and "))));
    }
    match chosen.first().copied() {
        Some("window") => {
            Ok(Subject { space: space.unwrap_or(Space::Model), window: window_of(&target["window"])?, drew: "window", object: None, view: None })
        }
        Some("objectId") => {
            let text =
                target["objectId"].as_str().ok_or_else(|| invalid(format!("`target.objectId` is {}: an entity handle", target["objectId"])))?;
            entity_subject(d, text, space.as_ref())
        }
        Some("path") => {
            let path = target["path"].as_str().unwrap_or_default();
            let handle = path.strip_prefix("entity:").ok_or_else(|| {
                invalid(format!("`target.path` {path:?} names no object a preview frames: give a diagnostic's `entity:<handle>`, or a `window`"))
            })?;
            entity_subject(d, handle, space.as_ref())
        }
        Some("view") => {
            let name = target["view"].as_str().ok_or_else(|| invalid(format!("`target.view` is {}: a named view's name", target["view"])))?;
            let view = d.views.iter().find(|v| v.name.eq_ignore_ascii_case(name)).ok_or_else(|| {
                let names: Vec<&str> = d.views.iter().map(|v| v.name.as_str()).collect();
                not_found(format!(
                    "the drawing has no named view `{name}` (its views: {})",
                    if names.is_empty() { "none".into() } else { names.join(", ") }
                ))
            })?;
            if space.as_ref().is_some_and(|s| *s != Space::Model) {
                return Err(invalid(format!("the named view `{}` is a view of Model space: leave `layout` out", view.name)));
            }
            let half = Vec2::new(view.width / 2.0, view.height / 2.0);
            let window = Bounds2::new(view.center - half, view.center + half);
            kition_cad_io::check_window(&window).map_err(|e| invalid(format!("the named view `{}`: {e}", view.name)))?;
            Ok(Subject { space: Space::Model, window, drew: "view", object: None, view: Some(view.name.clone()) })
        }
        _ => {
            let space = space.unwrap_or(Space::Model);
            let extents = d.extents(&space);
            let sheet = match &space {
                Space::Paper(name) => kition_cad_render::sheet(d, name).map(|s| s.bounds()),
                Space::Model => None,
            };
            let all = match (extents.is_empty(), sheet) {
                (false, Some(s)) => extents.union(&s),
                (true, Some(s)) => s,
                (false, None) => extents,
                (true, None) => {
                    let lo = d.header.point("LIMMIN").map(|p| p.xy()).unwrap_or(Vec2::ZERO);
                    let hi = d.header.point("LIMMAX").map(|p| p.xy()).unwrap_or(Vec2::new(420.0, 297.0));
                    Bounds2::new(lo, hi)
                }
            };
            Ok(Subject { space, window: with_margin(all), drew: "extents", object: None, view: None })
        }
    }
}

/// The drawing to draw: `d` as it is, or with only the layers `params.layers` names shown.
fn layered<'d>(d: &'d Drawing, params: &Value) -> Result<std::borrow::Cow<'d, Drawing>, String> {
    let Some(value) = params.get("layers").filter(|v| !v.is_null()) else { return Ok(std::borrow::Cow::Borrowed(d)) };
    let names: Vec<&str> = value
        .as_array()
        .and_then(|a| a.iter().map(Value::as_str).collect())
        .ok_or_else(|| invalid(format!("`layers` is {value}: a list of layer names")))?;
    if let Some(missing) = names.iter().find(|name| d.layer(name).is_none()) {
        return Err(not_found(format!("the drawing has no layer `{missing}`; inspect's `layers` lists them")));
    }
    let mut only = d.clone();
    for layer in &mut only.layers {
        if !names.iter().any(|name| name.eq_ignore_ascii_case(&layer.name)) {
            layer.on = false;
        }
    }
    Ok(std::borrow::Cow::Owned(only))
}

impl CadDocument {
    /// `file.render`, as the module docs say.
    pub(crate) fn render_view(&self, params: &Value) -> Result<Value, String> {
        let d = self.session.doc().map_err(crate::engine_error)?;
        let max_size = match params.get("maxSize") {
            None | Some(Value::Null) => DEFAULT_SIZE,
            Some(value) => value
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .filter(|n| (1..=MAX_SIZE).contains(n))
                .ok_or_else(|| invalid(format!("`maxSize` must be a whole number from 1 to {MAX_SIZE}, not {value}")))?,
        };
        let asked = match params.get("scale") {
            None | Some(Value::Null) => None,
            Some(value) => Some(
                value
                    .as_f64()
                    .filter(|s| s.is_finite() && *s > 0.0)
                    .ok_or_else(|| invalid(format!("`scale` must be a number above 0, not {value}")))?,
            ),
        };
        let subject = subject(d, params)?;
        let drawing = layered(d, params)?;
        let window = subject.window;
        let longest = window.width().max(window.height());
        let fit = f64::from(max_size) / longest;
        let scale = asked.map_or(fit, |s| s.min(fit));
        if !(scale.is_finite() && scale > 0.0) {
            return Err(invalid("the view cannot be drawn at any scale"));
        }
        let pixels = |side: f64| (side * scale).round().clamp(1.0, f64::from(MAX_SIZE)) as u32;
        let (width, height) = (pixels(window.width()), pixels(window.height()));
        let center = window.center();
        let half = Vec2::new(f64::from(width) / (2.0 * scale), f64::from(height) / (2.0 * scale));
        let drawn = Bounds2::new(center - half, center + half);
        // Curves flattened to about a quarter of a pixel.
        let options = kition_cad_render::Options { tolerance: 0.25 / scale, view_height: drawn.height(), ..Default::default() };
        let list = kition_cad_render::build(&drawing, &subject.space, &options);
        let view = cad_raster::View { center, scale, width, height };
        let raster = RasterOptions { background: PAPER, hairline: 1.0, ..RasterOptions::default() };
        let png = cad_raster::render_png(&list, &view, &raster).ok_or_else(|| format!("cannot draw {width} x {height} pixels"))?;
        let mut answer = json!({
            "png": kition_cad_engine::cmd::file::base64_encode(&png),
            "mimeType": "image/png",
            "width": width,
            "height": height,
            "scale": scale,
            "layout": context::space_name(&subject.space),
            "window": [drawn.min.x, drawn.min.y, drawn.max.x, drawn.max.y],
            "unitsPerPixel": 1.0 / scale,
            "drew": subject.drew,
        });
        if let Some(object) = subject.object {
            answer["objectId"] = json!(object);
        }
        if let Some(view) = subject.view {
            answer["view"] = json!(view);
        }
        Ok(answer)
    }
}
