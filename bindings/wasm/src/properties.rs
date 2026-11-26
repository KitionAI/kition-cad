//! The editor's Properties panel and the details of its layer list (walkthrough N13, N14):
//! `object.describeProperties` describes the selected objects as rows in groups (General: colour,
//! layer, linetype, linetype scale, lineweight, transparency; then each kind's own geometry, text,
//! pattern…), or, with nothing selected, the current properties new objects get. Each row says
//! what it holds (a number with its unit, text, a choice, a colour), its value as the drawing's
//! units show it, whether the selected objects disagree on it, and how a new value is written:
//! the existing command and parameter (`object.setProperties` for objects, `property.color`,
//! `layer.setCurrent`… for the current properties), so the panel changes the drawing through the
//! same commands a person or an agent runs, one undo step each.
//!
//! It also answers the drawing's layers with their colour, linetype and lineweight and why one
//! cannot be deleted, the loaded and the library linetypes, the hatch patterns, and reads values
//! typed in the panel (`parse`: arithmetic, feet and inches, a unit after the number, angles in
//! the drawing's angle units). It changes nothing.
use kition_cad_color::{Color, aci_rgb};
use kition_cad_doc::{Drawing, Entity, EntityKind, Lineweight, Transparency};

use super::*;

/// The read command the Properties panel and the layer list are drawn from.
pub const DESCRIBE: &str = "object.describeProperties";

/// The catalogue entry of [`DESCRIBE`].
pub(super) fn command() -> KitCommand {
    KitCommand {
        id: DESCRIBE.into(),
        title: "Describe Properties".into(),
        params: Some(r#"{handles?: [string], parse?: [{text: string, unit?: "length"|"angle"|"number"}]}"#.into()),
        mutates: false,
        enabled: true,
        ..Default::default()
    }
    .with_kit_schema()
}

/// The rows whose value must be above zero.
const POSITIVE: &[&str] = &["radius", "diameter", "circumference", "area", "height", "scale", "widthFactor", "ltscale", "textHeight"];

/// The standard colours the colour choices offer after ByLayer and ByBlock (ACI 1 to 7).
const STANDARD_COLORS: std::ops::RangeInclusive<u8> = 1..=7;
/// The most objects described one by one; a larger selection shows its general rows from the
/// first ones (their values may then differ from objects further on without saying so).
const MOST_OBJECTS: usize = 5000;

/// How the drawing shows lengths and angles (UNITS).
#[derive(Clone, Copy)]
struct Units {
    lunits: i64,
    luprec: i64,
    aunits: i64,
    auprec: i64,
    insunits: i64,
}

impl Units {
    fn of(d: &Drawing) -> Self {
        let h = &d.header;
        Units {
            lunits: h.i64("LUNITS", 2),
            luprec: h.i64("LUPREC", 4),
            aunits: h.i64("AUNITS", 0),
            auprec: h.i64("AUPREC", 0),
            insunits: h.i64("INSUNITS", 0),
        }
    }

    /// A length as the drawing's units show it, rounded to LUPREC places; in decimal units the
    /// trailing zeros are left off ("30", "340.4116"), as a value typed would be written.
    fn length(&self, v: f64) -> String {
        let text = kition_cad_engine::units::format_distance(v, self.lunits, self.luprec);
        if self.lunits == 2 { trim_zeros(text) } else { text }
    }

    /// An angle given in degrees, in the drawing's angle units (AUNITS, AUPREC).
    fn angle(&self, degrees: f64) -> String {
        let text = kition_cad_engine::units::format_angle(degrees.to_radians(), self.aunits, self.auprec);
        if self.aunits == 0 { trim_zeros(text) } else { text }
    }

    /// A plain number (a scale, a count), to six places without trailing zeros.
    fn plain(&self, v: f64) -> String {
        if !v.is_finite() {
            return "0".into();
        }
        trim_zeros(format!("{v:.6}"))
    }

    /// Drawing units per `unit` typed after a number (`10 mm`, `2ft`), when the drawing says
    /// what its unit is (INSUNITS); none for a unit it cannot convert.
    fn per(&self, unit: &str) -> Option<f64> {
        let millimetres = |code: i64| match code {
            1 => Some(25.4),
            2 => Some(304.8),
            4 => Some(1.0),
            5 => Some(10.0),
            6 => Some(1000.0),
            _ => None,
        };
        let typed = match unit {
            "mm" => 4,
            "cm" => 5,
            "m" => 6,
            "in" | "inch" | "inches" => 1,
            "ft" | "feet" | "foot" => 2,
            _ => return None,
        };
        Some(millimetres(typed)? / millimetres(self.insunits)?)
    }
}

/// "30.0000" → "30", "12.5000" → "12.5", "-0.0000" → "0".
fn trim_zeros(text: String) -> String {
    if !text.contains('.') || text.contains(['E', 'e']) {
        return text;
    }
    let trimmed = text.trim_end_matches('0').trim_end_matches('.');
    if trimmed == "-0" || trimmed.is_empty() { "0".into() } else { trimmed.to_owned() }
}

/// What a row's number measures, for showing and reading it.
#[derive(Clone, Copy, PartialEq)]
enum Unit {
    Length,
    Angle,
    Area,
    Number,
}

impl Unit {
    fn name(self) -> &'static str {
        match self {
            Unit::Length => "length",
            Unit::Angle => "angle",
            Unit::Area => "area",
            Unit::Number => "number",
        }
    }
}

/// One row of the panel.
struct Row {
    group: &'static str,
    key: String,
    label: &'static str,
    /// `number`, `text`, `choice`, `color` or `boolean`.
    kind: &'static str,
    unit: Option<Unit>,
    value: Value,
    /// How a new value is written: `{command, params, key}`, plus `point` and `index` for one
    /// coordinate of a point (the other is kept). None: the row cannot be changed here.
    write: Option<Value>,
    /// The colour a colour row shows (resolved through the layer for ByLayer).
    rgb: Option<String>,
}

/// The rows of one object, in order.
struct Rows<'a> {
    rows: Vec<Row>,
    handles: &'a [String],
}

impl Rows<'_> {
    /// The write of `object.setProperties` with `key` for the objects described.
    fn set(&self, key: &str) -> Value {
        json!({ "command": "object.setProperties", "params": { "handles": self.handles }, "key": key })
    }

    fn push(
        &mut self,
        group: &'static str,
        key: &str,
        label: &'static str,
        kind: &'static str,
        unit: Option<Unit>,
        value: Value,
        write: Option<Value>,
    ) {
        self.rows.push(Row { group, key: key.to_owned(), label, kind, unit, value, write, rgb: None });
    }

    fn number(&mut self, group: &'static str, key: &str, label: &'static str, unit: Unit, value: f64, write: Option<&str>) {
        let write = write.map(|param| self.set(param));
        self.push(group, key, label, "number", Some(unit), json!(finite(value)), write);
    }

    /// A point's X and Y rows (`key.x`, `key.y`); `write` names the point parameter both write.
    fn point(&mut self, group: &'static str, key: &str, labels: [&'static str; 2], at: Vec2, write: Option<&str>) {
        for (index, (label, value)) in labels.into_iter().zip([at.x, at.y]).enumerate() {
            let write = write.map(|param| {
                let mut w = self.set(param);
                w["point"] = json!([finite(at.x), finite(at.y)]);
                w["index"] = json!(index);
                w
            });
            self.push(
                group,
                &format!("{key}.{}", if index == 0 { "x" } else { "y" }),
                label,
                "number",
                Some(Unit::Length),
                json!(finite(value)),
                write,
            );
        }
    }

    fn text(&mut self, group: &'static str, key: &str, label: &'static str, value: &str, write: Option<&str>) {
        let write = write.map(|param| self.set(param));
        self.push(group, key, label, "text", None, json!(value), write);
    }

    fn choice(&mut self, group: &'static str, key: &str, label: &'static str, value: Value, write: Option<&str>) {
        let write = write.map(|param| self.set(param));
        self.push(group, key, label, "choice", None, value, write);
    }
}

fn finite(v: f64) -> f64 {
    if v.is_finite() { v } else { 0.0 }
}

/// A lineweight as the choices hold it: "ByLayer", "ByBlock", "Default", or millimetres.
fn lineweight_value(w: Lineweight) -> Value {
    match w {
        Lineweight::Mm100(v) => json!(f64::from(v) / 100.0),
        other => json!(other.name()),
    }
}

fn transparency_value(t: Transparency) -> Value {
    match t {
        Transparency::Percent(p) => json!(p.to_string()),
        other => json!(other.name()),
    }
}

/// The colour an object or layer colour shows as, `#rrggbb`.
fn rgb_of(d: &Drawing, color: Color, layer: &str) -> String {
    let layer = d.layer(layer).map_or(Color::Index(7), |l| l.color);
    color.resolve(layer, Color::Index(7)).hex()
}

/// The general rows every object has, and its kind's own.
fn object_rows(d: &Drawing, e: &Entity, rows: &mut Rows<'_>) {
    let c = &e.common;
    rows.push("General", "color", "Color", "color", None, json!(c.color.name()), Some(rows.set("color")));
    if let Some(last) = rows.rows.last_mut() {
        last.rgb = Some(rgb_of(d, c.color, &c.layer));
    }
    rows.choice("General", "layer", "Layer", json!(c.layer), Some("layer"));
    rows.choice("General", "linetype", "Linetype", json!(c.linetype), Some("linetype"));
    rows.number("General", "ltscale", "Linetype scale", Unit::Number, c.ltscale, Some("ltscale"));
    rows.choice("General", "lineweight", "Lineweight", lineweight_value(c.lineweight), Some("lineweight"));
    rows.text("General", "transparency", "Transparency", transparency_value(c.transparency).as_str().unwrap_or("ByLayer"), Some("transparency"));
    match &e.kind {
        EntityKind::Line(l) => {
            let (a, b) = (l.a.xy(), l.b.xy());
            rows.point("Geometry", "start", ["Start X", "Start Y"], a, Some("start"));
            rows.point("Geometry", "end", ["End X", "End Y"], b, Some("end"));
            rows.number("Geometry", "delta.x", "Delta X", Unit::Length, b.x - a.x, None);
            rows.number("Geometry", "delta.y", "Delta Y", Unit::Length, b.y - a.y, None);
            rows.number("Geometry", "length", "Length", Unit::Length, a.dist(b), Some("length"));
            rows.number("Geometry", "angle", "Angle", Unit::Angle, a.angle_to(b).to_degrees(), Some("angle"));
        }
        EntityKind::Circle(k) => {
            rows.point("Geometry", "center", ["Center X", "Center Y"], k.center.xy(), Some("center"));
            rows.number("Geometry", "radius", "Radius", Unit::Length, k.radius, Some("radius"));
            rows.number("Geometry", "diameter", "Diameter", Unit::Length, k.radius * 2.0, Some("diameter"));
            rows.number("Geometry", "circumference", "Circumference", Unit::Length, k.radius * kition_cad_geom::TAU, Some("circumference"));
            rows.number("Geometry", "area", "Area", Unit::Area, k.radius * k.radius * kition_cad_geom::PI, Some("area"));
        }
        EntityKind::Arc(k) => {
            let sweep = kition_cad_geom::ccw_sweep(k.start, k.end);
            rows.point("Geometry", "center", ["Center X", "Center Y"], k.center.xy(), Some("center"));
            rows.number("Geometry", "radius", "Radius", Unit::Length, k.radius, Some("radius"));
            rows.number("Geometry", "startAngle", "Start angle", Unit::Angle, k.start.to_degrees(), Some("startAngle"));
            rows.number("Geometry", "endAngle", "End angle", Unit::Angle, k.end.to_degrees(), Some("endAngle"));
            rows.number("Geometry", "totalAngle", "Total angle", Unit::Angle, sweep.to_degrees(), None);
            rows.number("Geometry", "arcLength", "Arc length", Unit::Length, k.radius * sweep, None);
        }
        EntityKind::LwPolyline(p) => {
            let pl = kition_cad_geom::Polyline { vertices: p.vertices.clone(), closed: p.closed };
            rows.number("Geometry", "vertices", "Vertices", Unit::Number, p.vertices.len() as f64, None);
            if let Some(first) = p.vertices.first() {
                rows.point("Geometry", "vertex", ["Vertex X", "Vertex Y"], first.p, None);
            }
            rows.number("Geometry", "width", "Global width", Unit::Length, p.const_width, Some("width"));
            rows.number("Geometry", "length", "Length", Unit::Length, pl.len(), None);
            if p.closed {
                rows.number("Geometry", "area", "Area", Unit::Area, pl.area().abs(), None);
            }
            rows.push("Misc", "closed", "Closed", "boolean", None, json!(p.closed), Some(rows.set("closed")));
        }
        EntityKind::Ellipse(k) => {
            rows.point("Geometry", "center", ["Center X", "Center Y"], k.center.xy(), None);
            rows.number("Geometry", "majorRadius", "Major radius", Unit::Length, k.major.xy().len(), None);
            rows.number("Geometry", "minorRadius", "Minor radius", Unit::Length, k.major.xy().len() * k.ratio, None);
        }
        EntityKind::Point(k) => rows.point("Geometry", "position", ["Position X", "Position Y"], k.p.xy(), None),
        EntityKind::Text(t) => {
            rows.text("Text", "text", "Contents", &t.value, Some("text"));
            rows.text("Text", "style", "Style", &t.style, None);
            rows.number("Text", "height", "Height", Unit::Length, t.height, Some("height"));
            rows.number("Text", "rotation", "Rotation", Unit::Angle, t.rotation.to_degrees(), Some("rotation"));
            rows.number("Text", "widthFactor", "Width factor", Unit::Number, t.width_factor, Some("widthFactor"));
            rows.point("Geometry", "position", ["Position X", "Position Y"], t.justify_point(), Some("position"));
        }
        EntityKind::MText(t) => {
            rows.text("Text", "text", "Contents", &t.contents.replace("\\P", "\n"), Some("text"));
            rows.text("Text", "style", "Style", &t.style, None);
            rows.number("Text", "height", "Height", Unit::Length, t.height, Some("height"));
            rows.number("Text", "rotation", "Rotation", Unit::Angle, t.rotation.to_degrees(), Some("rotation"));
            rows.number("Text", "width", "Width", Unit::Length, t.width, Some("width"));
            rows.point("Geometry", "position", ["Position X", "Position Y"], t.insert.xy(), None);
        }
        EntityKind::Dimension(k) => {
            rows.choice("Text", "dimStyle", "Dimension style", json!(k.style), Some("dimStyle"));
            rows.number("Text", "measurement", "Measurement", Unit::Length, k.measurement, None);
            rows.text("Text", "textOverride", "Text override", &k.text, Some("textOverride"));
        }
        EntityKind::Hatch(h) => {
            rows.choice("Pattern", "pattern", "Pattern", json!(h.pattern), Some("pattern"));
            rows.number("Pattern", "scale", "Scale", Unit::Number, h.scale, Some("scale"));
            rows.number("Pattern", "angle", "Angle", Unit::Angle, h.angle.to_degrees(), Some("angle"));
        }
        EntityKind::Insert(i) => {
            rows.text("Misc", "block", "Name", &i.block, None);
            rows.point("Geometry", "position", ["Position X", "Position Y"], i.insert.xy(), None);
            rows.number("Geometry", "scale", "Scale", Unit::Number, i.scale.x, Some("scale"));
            rows.number("Geometry", "rotation", "Rotation", Unit::Angle, i.rotation.to_degrees(), Some("rotation"));
        }
        _ => {}
    }
}

/// Whether two values of a row agree (numbers to a part in a billion).
fn same(a: &Value, b: &Value) -> bool {
    match (a.as_f64(), b.as_f64()) {
        (Some(x), Some(y)) => (x - y).abs() <= 1e-9 * x.abs().max(y.abs()).max(1.0),
        _ => a == b,
    }
}

/// The text a row shows for `value`.
fn display(units: &Units, row: &Row) -> String {
    match (&row.value, row.unit) {
        (Value::Null, _) => String::new(),
        (Value::Number(n), Some(unit)) => {
            let v = n.as_f64().unwrap_or(0.0);
            match unit {
                Unit::Length => units.length(v),
                Unit::Angle => units.angle(v),
                Unit::Area => units.length(v),
                Unit::Number => units.plain(v),
            }
        }
        (Value::Number(n), None) => n.as_f64().map_or_else(String::new, |v| format!("{v:.2} mm")),
        (Value::Bool(b), _) => if *b { "Yes" } else { "No" }.into(),
        (Value::String(s), _) => s.clone(),
        (other, _) => other.to_string(),
    }
}

/// The current properties new objects get (nothing selected), each written by its own command.
fn current_rows(d: &Drawing) -> Vec<Row> {
    let h = &d.header;
    let mut color = Color::from_aci(h.i64("CECOLOR", 256) as i16);
    if let Some(rgb) = h.get("CECOLOR_RGB").and_then(|v| v.as_i64()) {
        color = Color::True(kition_cad_color::Rgb::from_u32(rgb as u32));
    }
    let layer = h.str("CLAYER", "0");
    let write = |command: &str, params: Value, key: &str| Some(json!({ "command": command, "params": params, "key": key }));
    let row = |key: &str, label: &'static str, kind: &'static str, unit: Option<Unit>, value: Value, write: Option<Value>| Row {
        group: "General",
        key: key.to_owned(),
        label,
        kind,
        unit,
        value,
        write,
        rgb: None,
    };
    vec![
        Row {
            rgb: Some(rgb_of(d, color, &layer)),
            ..row("color", "Color", "color", None, json!(color.name()), write("property.color", json!({}), "color"))
        },
        row("layer", "Layer", "choice", None, json!(layer), write("layer.setCurrent", json!({}), "name")),
        row("linetype", "Linetype", "choice", None, json!(h.str("CELTYPE", "ByLayer")), write("property.linetype", json!({}), "current")),
        row(
            "ltscale",
            "Linetype scale",
            "number",
            Some(Unit::Number),
            json!(finite(h.f64("CELTSCALE", 1.0))),
            write("variable.set", json!({ "name": "CELTSCALE" }), "value"),
        ),
        row(
            "lineweight",
            "Lineweight",
            "choice",
            None,
            lineweight_value(Lineweight::from_dxf(h.i64("CELWEIGHT", -1) as i16)),
            write("property.lineweight", json!({}), "lineweight"),
        ),
        Row {
            group: "Text",
            ..row("textStyle", "Text style", "choice", None, json!(h.str("TEXTSTYLE", "Standard")), write("textStyle.setCurrent", json!({}), "name"))
        },
        Row {
            group: "Text",
            ..row(
                "textHeight",
                "Text height",
                "number",
                Some(Unit::Length),
                json!(finite(h.f64("TEXTSIZE", 0.2))),
                write("variable.set", json!({ "name": "TEXTSIZE" }), "value"),
            )
        },
        Row {
            group: "Text",
            ..row(
                "dimStyle",
                "Dimension style",
                "choice",
                None,
                json!(h.str("DIMSTYLE", "Standard")),
                write("dimensionStyle.setCurrent", json!({}), "name"),
            )
        },
    ]
}

/// The choices of a choice or colour row, by its key.
fn options(d: &Drawing, key: &str) -> Option<Value> {
    let option = |value: Value, label: String| json!({ "value": value, "label": label });
    let list: Vec<Value> = match key {
        "color" => {
            let mut list = vec![option(json!("ByLayer"), "ByLayer".into()), option(json!("ByBlock"), "ByBlock".into())];
            list.extend(STANDARD_COLORS.map(|i| {
                let color = Color::Index(i);
                json!({ "value": color.name(), "label": color.name(), "rgb": aci_rgb(i).hex() })
            }));
            list
        }
        "layer" => d.layers.iter().map(|l| option(json!(l.name), l.name.clone())).collect(),
        "linetype" => {
            let mut list = vec![option(json!("ByLayer"), "ByLayer".into()), option(json!("ByBlock"), "ByBlock".into())];
            list.extend(
                d.linetypes
                    .iter()
                    .filter(|l| !["bylayer", "byblock"].contains(&l.name.to_ascii_lowercase().as_str()))
                    .map(|l| option(json!(l.name), l.name.clone())),
            );
            // The library's linetypes not loaded yet: choosing one loads it first.
            list.extend(
                kition_cad_doc::library::standard_linetypes()
                    .into_iter()
                    .filter(|l| d.linetype(&l.name).is_none())
                    .map(|l| json!({ "value": l.name, "label": l.name, "load": true })),
            );
            list
        }
        "lineweight" => {
            let mut list = vec![
                option(json!("ByLayer"), "ByLayer".into()),
                option(json!("ByBlock"), "ByBlock".into()),
                option(json!("Default"), "Default".into()),
            ];
            list.extend(Lineweight::STANDARD.iter().map(|v| option(json!(f64::from(*v) / 100.0), Lineweight::Mm100(*v).name())));
            list
        }
        "dimStyle" => d.dim_styles.iter().map(|s| option(json!(s.name), s.name.clone())).collect(),
        "textStyle" => d.text_styles.iter().map(|s| option(json!(s.name), s.name.clone())).collect(),
        "pattern" => kition_cad_doc::library::standard_patterns().into_iter().map(|p| option(json!(p.name), p.name.to_owned())).collect(),
        "closed" => vec![option(json!(true), "Yes".into()), option(json!(false), "No".into())],
        _ => return None,
    };
    Some(Value::Array(list))
}

/// The drawing's layers as the layer list shows them, and why each cannot be deleted, if so:
/// `reserved` (0, Defpoints), `current`, or `used` (objects are on it).
fn layers(d: &Drawing) -> Vec<Value> {
    let current = d.header.str("CLAYER", "0");
    let mut used = std::collections::HashSet::new();
    let entities = d.model.iter().chain(d.layouts.iter().flat_map(|l| l.entities.iter())).chain(d.blocks.values().flat_map(|b| b.entities.iter()));
    for e in entities {
        used.insert(e.common.layer.to_ascii_lowercase());
    }
    d.layers
        .iter()
        .map(|l| {
            let is_current = l.name.eq_ignore_ascii_case(&current);
            let blocked = if l.name == "0" || l.name.eq_ignore_ascii_case("Defpoints") {
                Some("reserved")
            } else if is_current {
                Some("current")
            } else if used.contains(&l.name.to_ascii_lowercase()) {
                Some("used")
            } else {
                None
            };
            json!({
                "name": l.name,
                "color": l.color.name(),
                "rgb": l.color.resolve(Color::Index(7), Color::Index(7)).hex(),
                "linetype": l.linetype,
                "lineweight": lineweight_value(l.lineweight),
                "transparency": l.transparency,
                "on": l.on,
                "frozen": l.frozen,
                "locked": l.locked,
                "plot": l.plot,
                "current": is_current,
                "deleteBlocked": blocked,
            })
        })
        .collect()
}

/// A value typed in the panel, as a number: a length (decimal, feet and inches `1'6"`, a unit
/// after it `25mm` when INSUNITS says what the drawing's unit is, or arithmetic `2*30+5`), an
/// angle in degrees (AUNITS, `30d`, `0.5r`, or arithmetic in degrees), or any number (arithmetic).
fn parse(session: &Session, units: &Units, text: &str, unit: &str) -> Option<f64> {
    let text = text.trim();
    if text.is_empty() || text.len() > 1000 {
        return None;
    }
    let calculated = || kition_cad_engine::cmd::calculate(text, &serde_json::Map::new()).ok();
    let value = match unit {
        "length" => kition_cad_engine::units::parse_distance(text)
            .or_else(|| {
                let split = text.find(|c: char| c.is_ascii_alphabetic())?;
                let (number, suffix) = text.split_at(split);
                let per = units.per(suffix.trim().to_ascii_lowercase().as_str())?;
                let number = kition_cad_engine::units::parse_distance(number)
                    .or_else(|| kition_cad_engine::cmd::calculate(number, &serde_json::Map::new()).ok())?;
                Some(number * per)
            })
            .or_else(calculated),
        "angle" => session.angle_settings().amount(text).map(f64::to_degrees).or_else(calculated),
        _ => calculated(),
    };
    value.filter(|v| v.is_finite())
}

impl CadDocument {
    /// `object.describeProperties`: see the module.
    pub(super) fn describe_properties(&self, params: &Value) -> Result<Value, String> {
        let d = self.session.doc().map_err(engine_error)?;
        let units = Units::of(d);
        let given: Option<Vec<kition_cad_doc::Handle>> = params
            .get("handles")
            .and_then(Value::as_array)
            .map(|list| list.iter().filter_map(Value::as_str).filter_map(kition_cad_doc::Handle::parse_hex).collect());
        let handles = given.unwrap_or_else(|| self.session.selection());
        let entities: Vec<&Entity> = handles.iter().filter_map(|h| d.entity(*h)).map(|e| &**e).collect();
        let names: Vec<String> = entities.iter().map(|e| e.handle.hex()).collect();
        let current = entities.is_empty();
        let mut types: Vec<(&'static str, usize)> = Vec::new();
        for e in &entities {
            let name = e.kind.type_name();
            match types.iter_mut().find(|(t, _)| *t == name) {
                Some((_, n)) => *n += 1,
                None => types.push((name, 1)),
            }
        }
        // A locked member keeps every row from being changed, as the layer lock keeps the objects.
        let locked = entities.iter().any(|e| d.layer(&e.common.layer).is_some_and(|l| l.locked));
        let mut rows = if current {
            current_rows(d)
        } else {
            let mut merged: Option<Vec<(Row, bool)>> = None;
            for e in entities.iter().take(MOST_OBJECTS) {
                let mut own = Rows { rows: Vec::new(), handles: &names };
                object_rows(d, e, &mut own);
                merged = Some(match merged {
                    None => own.rows.into_iter().map(|row| (row, false)).collect(),
                    // The rows every object has, in the first one's order, marked where they disagree.
                    Some(rows) => rows
                        .into_iter()
                        .filter_map(|(row, varies)| {
                            let other = own.rows.iter().find(|o| o.key == row.key && o.kind == row.kind)?;
                            let varies = varies || !same(&row.value, &other.value);
                            Some((row, varies))
                        })
                        .collect(),
                });
            }
            // Only the general rows when the objects are of several kinds.
            let one_kind = types.len() == 1;
            let merged = merged.unwrap_or_default();
            let varied: Vec<String> = merged.iter().filter(|(_, varies)| *varies).map(|(row, _)| row.key.clone()).collect();
            merged
                .into_iter()
                .filter(|(row, _)| one_kind || row.group == "General")
                .map(|(mut row, varies)| {
                    if varies {
                        row.value = Value::Null;
                        row.rgb = None;
                    }
                    // One coordinate of a point is written with the other kept: not where the
                    // objects disagree on the other.
                    if let Some(companion) =
                        row.key.strip_suffix(".x").map(|k| format!("{k}.y")).or_else(|| row.key.strip_suffix(".y").map(|k| format!("{k}.x")))
                        && varied.contains(&companion)
                    {
                        row.write = None;
                    }
                    row
                })
                .collect()
        };
        if locked {
            for row in &mut rows {
                row.write = None;
            }
        }
        let rows: Vec<Value> = rows
            .iter()
            .map(|row| {
                let mut value = json!({
                    "group": row.group,
                    "key": row.key,
                    "label": row.label,
                    "kind": row.kind,
                    "value": row.value,
                    "display": display(&units, row),
                    "varies": row.value.is_null(),
                    "editable": row.write.is_some(),
                });
                // A size, a scale or a height must stay above zero (the writers ignore the rest).
                if POSITIVE.contains(&row.key.as_str()) {
                    value["positive"] = json!(true);
                }
                if let Some(unit) = row.unit {
                    value["unit"] = json!(unit.name());
                }
                if let Some(write) = &row.write {
                    value["write"] = write.clone();
                }
                if let Some(rgb) = &row.rgb {
                    value["rgb"] = json!(rgb);
                }
                if let Some(options) = options(d, &row.key).filter(|_| matches!(row.kind, "choice" | "color" | "boolean")) {
                    value["options"] = options;
                }
                value
            })
            .collect();
        let parsed: Vec<Value> = params
            .get("parse")
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .take(100)
                    .map(|item| {
                        let text = item.get("text").and_then(Value::as_str).unwrap_or_default();
                        let unit = item.get("unit").and_then(Value::as_str).unwrap_or("number");
                        parse(&self.session, &units, text, unit).map_or(Value::Null, |v| json!(v))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut out = json!({
            "count": entities.len(),
            "current": current,
            "types": types.iter().map(|(name, count)| json!({ "type": name, "count": count })).collect::<Vec<_>>(),
            "locked": locked,
            "rows": rows,
            "layers": layers(d),
            "linetypes": d.linetypes.iter().map(|l| l.name.clone()).collect::<Vec<_>>(),
            "library": kition_cad_doc::library::standard_linetypes().into_iter().filter(|l| d.linetype(&l.name).is_none()).map(|l| l.name).collect::<Vec<_>>(),
            "patterns": kition_cad_doc::library::standard_patterns().into_iter().map(|p| p.name).collect::<Vec<_>>(),
            "colors": STANDARD_COLORS.map(|i| json!({ "value": Color::Index(i).name(), "rgb": aci_rgb(i).hex() })).collect::<Vec<_>>(),
            "lineweights": options(d, "lineweight"),
        });
        // One object edited in place (Edit Text in the drawing menu): what a double-click on it asks.
        if let [one] = entities.as_slice()
            && let Some(request) = in_place::request(d, one)
        {
            out["inPlace"] = request;
        }
        if params.get("parse").is_some() {
            out["parsed"] = Value::Array(parsed);
        }
        Ok(out)
    }
}
