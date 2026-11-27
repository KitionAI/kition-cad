//! Dynamic input (F12, DYNMODE): the value boxes the editor draws beside the cursor while a command
//! asks for a point.
//!
//! The command line's text is the one source of truth and the boxes show it
//! (`web/src/dynamicInput.ts`). The editor tells the engine what is typed in the boxes
//! ([`SET_ENTRY`]) so a locked value holds the cursor here, in the cursor pipeline, where clicks and
//! the preview read it:
//! - `40` is typed in the first box: X at a first point, else the distance from the base point
//!   (later points read as distance and angle, measured from the last point).
//! - `40,` locks X = 40 (or a relative X) and the cursor only moves Y; `40<` locks the distance and
//!   the cursor only turns the direction. With Tab the editor locks the second value as well
//!   (`second_locked`), which pins the cursor to the point; a locked second value alone (the first
//!   box emptied again) holds Y or the direction.
//! - Enter fills an empty box from the cursor and sends the point measured from the last point
//!   (`@40<30`), or absolute at a first point. A plain number is sent as typed: direct distance.
//! - `@`, `#` entries, options and names are not box entries and go through as typed.
//!
//! The live values the editor shows dimmed in empty boxes come with the cursor's overlays
//! ([`CadDocument::dynamic_overlay`]): a fully transparent label, so a host drawing overlays plainly
//! shows nothing for it.
use super::*;

/// The editor command that tells the engine what is typed in the dynamic input boxes.
pub const SET_ENTRY: &str = "drafting.setDynamicEntry";
/// The tag of the live values' label: fully transparent (as [`CURSOR`]), told apart by its colour.
pub const DYNAMIC: [u8; 4] = [255, 255, 255, 0];
/// The most characters of an entry the engine keeps.
const ENTRY_CHARS: usize = 200;

/// What the editor says is typed in the boxes.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct DynamicEntry {
    pub text: String,
    /// Tab locked the second value too.
    pub second_locked: bool,
}

/// How the boxes read a point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Format {
    /// X and Y.
    Cartesian,
    /// Distance and angle.
    Polar,
}

/// An entry read from the boxes' text.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Entry<'a> {
    pub format: Format,
    /// The first box's text (X or the distance).
    pub first: &'a str,
    /// A separator was typed: the first value is done.
    pub split: bool,
    /// The second box's text (Y or the angle).
    pub second: &'a str,
}

/// The prompt the boxes work for: its base point (the last point), if it has one. Values are
/// measured from it (relative); without one they are absolute.
#[derive(Clone, Copy, Debug)]
pub(super) struct Frame {
    pub base: Option<Vec2>,
    pub angles: kition_cad_engine::units::AngleSettings,
}

impl Frame {
    fn origin(&self) -> Vec2 {
        self.base.unwrap_or(Vec2::ZERO)
    }

    /// What the boxes show before a separator is typed: distance and angle from a base point,
    /// else X and Y.
    fn default_format(&self) -> Format {
        if self.base.is_some() { Format::Polar } else { Format::Cartesian }
    }

    /// `point`'s values in `format`: (X, Y) from the origin, or (distance, angle in radians
    /// counterclockwise from +X).
    fn values(&self, format: Format, point: Vec2) -> (f64, f64) {
        let d = point - self.origin();
        match format {
            Format::Cartesian => (d.x, d.y),
            Format::Polar => (d.len(), d.y.atan2(d.x)),
        }
    }

    /// The second box's typed value: a distance (Y), or a direction in radians as the drawing
    /// reads typed angles (AUNITS, ANGBASE, ANGDIR).
    fn second_value(&self, entry: &Entry<'_>) -> Option<f64> {
        let text = entry.second.trim();
        if text.is_empty() {
            return None;
        }
        match entry.format {
            Format::Cartesian => kition_cad_engine::units::parse_distance(text),
            Format::Polar => self.angles.direction(text),
        }
        .filter(|value| value.is_finite())
    }

    /// The locked first value, once a separator follows it.
    fn first_locked(entry: &Entry<'_>) -> Option<f64> {
        if entry.split { kition_cad_engine::units::parse_distance(entry.first).filter(|value| value.is_finite()) } else { None }
    }

    /// Reads `text` as a box entry, or `None` when it is something else (an `@` or `#` entry, an
    /// option, a name, a 3D point).
    pub fn parse<'a>(&self, text: &'a str) -> Option<Entry<'a>> {
        let text = text.trim();
        if text.starts_with('@') || text.starts_with('#') {
            return None;
        }
        let entry = match (text.find(','), text.find('<')) {
            (Some(comma), lt) if lt.is_none_or(|lt| comma < lt) => {
                Entry { format: Format::Cartesian, first: text.get(..comma)?.trim(), split: true, second: text.get(comma + 1..)?.trim() }
            }
            (_, Some(lt)) => Entry { format: Format::Polar, first: text.get(..lt)?.trim(), split: true, second: text.get(lt + 1..)?.trim() },
            _ => Entry { format: self.default_format(), first: text, split: false, second: "" },
        };
        if !entry.first.is_empty() && kition_cad_engine::units::parse_distance(entry.first).is_none() {
            return None;
        }
        if entry.second.contains([',', '<']) || (!entry.second.is_empty() && self.second_value(&entry).is_none()) {
            return None;
        }
        Some(entry)
    }

    /// Whether the entry holds the cursor: a locked first value, or a Tab-locked second one.
    pub fn holds(&self, entry: &Entry<'_>, second_locked: bool) -> bool {
        Self::first_locked(entry).is_some() || (second_locked && self.second_value(entry).is_some())
    }

    /// `point` held to the entry's locked values: X (or the distance) once a separator follows
    /// it, both values once Tab locked the second, the second alone when the first box is empty.
    pub fn constrain(&self, entry: &Entry<'_>, point: Vec2, second_locked: bool) -> Vec2 {
        let origin = self.origin();
        let second = self.second_value(entry).filter(|_| second_locked);
        let held = match (Self::first_locked(entry), second, entry.format) {
            (Some(x), Some(y), Format::Cartesian) => origin + Vec2::new(x, y),
            (Some(d), Some(a), Format::Polar) => origin + Vec2::from_angle(a) * d,
            (Some(x), None, Format::Cartesian) => Vec2::new(origin.x + x, point.y),
            (Some(d), None, Format::Polar) => {
                let to = point - origin;
                let length = to.len();
                let direction = if length > 1e-12 { to * (1.0 / length) } else { Vec2::new(1.0, 0.0) };
                origin + direction * d
            }
            (None, Some(y), Format::Cartesian) => Vec2::new(point.x, origin.y + y),
            (None, Some(a), Format::Polar) => origin + Vec2::from_angle(a) * (point - origin).len(),
            (None, None, _) => point,
        };
        if held.is_finite() { held } else { point }
    }

    /// The command-line text Enter sends for `entry` with the cursor at `point`, or `None` to send
    /// the text as typed (nothing split: a plain number stays direct distance entry). An empty box
    /// takes the cursor's value. Measured from the last point (`last`, what `@` reads against) it is
    /// sent as `@x,y` or `@d<a`; from another base, or absolute, as the finished point `x,y`.
    pub fn complete(&self, entry: &Entry<'_>, point: Vec2, last: Vec2) -> Option<String> {
        if !entry.split {
            return None;
        }
        let (live_first, live_second) = self.values(entry.format, point);
        let typed_first = Self::first_locked(entry);
        let typed_second = self.second_value(entry);
        if self.base.is_none() && typed_first.is_some() && typed_second.is_some() {
            return None;
        }
        let first = typed_first.unwrap_or(live_first);
        let second = typed_second.unwrap_or(live_second);
        if !first.is_finite() || !second.is_finite() {
            return None;
        }
        if self.base.is_some_and(|base| base.near(last, 1e-9)) {
            let first_text = if typed_first.is_some() { entry.first.to_owned() } else { first.to_string() };
            return Some(match (entry.format, typed_second.is_some()) {
                (Format::Cartesian, true) => format!("@{first_text},{}", entry.second),
                (Format::Cartesian, false) => format!("@{first_text},{second}"),
                // Typed, the angle reads as typed angles do; the cursor's is degrees from +X.
                (Format::Polar, true) => format!("@{first_text}<{}", entry.second),
                (Format::Polar, false) => format!("@{first_text}<<{}", second.to_degrees()),
            });
        }
        let point = match entry.format {
            Format::Cartesian => self.origin() + Vec2::new(first, second),
            Format::Polar => self.origin() + Vec2::from_angle(second) * first,
        };
        point.is_finite().then(|| format!("{},{}", point.x, point.y))
    }
}

impl CadDocument {
    /// The prompt the boxes work for: a hot grip's (STRETCH and MOVE measure from the grip, ROTATE
    /// takes an angle, SCALE a factor), or the running command's; none with dynamic input off or a
    /// selection window open.
    fn dynamic_prompt(&self) -> Option<kition_cad_engine::Prompt> {
        if !self.session.settings.dynmode || self.session.pending_window.is_some() {
            return None;
        }
        if self.hot_grip.is_some() {
            return self.grip_prompt();
        }
        self.session.current_prompt()
    }

    /// The prompt the boxes work for, when they show two: dynamic input on, a command or a hot grip
    /// asking for a point (not a value or an object pick), no selection window open.
    pub(super) fn dynamic_frame(&self) -> Option<Frame> {
        let prompt = self.dynamic_prompt()?;
        (kition_cad_engine::pointmod::accepts(&prompt) && prompt.kind() == kition_cad_engine::prompt::PromptKind::Point)
            .then(|| Frame { base: prompt.base, angles: self.session.angle_settings() })
    }

    /// `point` held to the locked values typed in the boxes, if any hold it.
    pub(super) fn dynamic_hold(&self, point: Vec2) -> Option<Vec2> {
        let frame = self.dynamic_frame()?;
        let entry = frame.parse(&self.dynamic_entry.text)?;
        frame.holds(&entry, self.dynamic_entry.second_locked).then(|| frame.constrain(&entry, point, self.dynamic_entry.second_locked))
    }

    /// [`SET_ENTRY`]: keeps what is typed in the boxes (`text`, and whether Tab locked the second
    /// value), runs the cursor pipeline again with it, and answers how it reads: whether it is a
    /// box entry, its format, and the text Enter sends for it now (`complete`; `null` sends the
    /// text as typed).
    pub(super) fn set_dynamic_entry(&mut self, params: &Value) -> Result<Value, String> {
        let text = params.get("text").and_then(Value::as_str).ok_or("text is required")?;
        let second_locked = params.get("secondLocked").and_then(Value::as_bool).unwrap_or(false);
        self.dynamic_entry = DynamicEntry { text: text.chars().take(ENTRY_CHARS).collect(), second_locked };
        if self.cursor.is_some() {
            self.resolve_cursor();
        }
        let Some(frame) = self.dynamic_frame() else { return Ok(json!({ "entry": false, "complete": null })) };
        let Some(entry) = frame.parse(&self.dynamic_entry.text) else { return Ok(json!({ "entry": false, "complete": null })) };
        let point = self.aim.or(self.cursor).unwrap_or(self.session.cursor);
        Ok(json!({
            "entry": true,
            "format": if entry.format == Format::Polar { "polar" } else { "cartesian" },
            "relative": frame.base.is_some(),
            "holds": frame.holds(&entry, second_locked),
            // A hot grip's `@` reads from the grip (its base point), not the last point given.
            "complete": frame.complete(&entry, point, if self.hot_grip.is_some() { frame.base.unwrap_or(self.session.last_point) } else { self.session.last_point }),
        }))
    }

    /// Forgets what was typed in the boxes: the entry was sent, a click used it up, or Escape.
    pub(super) fn forget_dynamic_entry(&mut self) {
        self.dynamic_entry = DynamicEntry::default();
    }

    /// The live values the boxes show dimmed, as a transparent label at the cursor: `cartesian`
    /// (X and Y; from the base point when there is one) and `polar` (the distance and the angle in
    /// degrees from the base point), in the drawing's units. Whenever dynamic input is on and the
    /// prompt takes a point (a value prompt's single box shows the distance or the angle), a hot
    /// grip's included: SCALE's factor is the distance from the grip.
    pub(super) fn dynamic_overlay(&self) -> Option<ToolOverlay> {
        let prompt = self.dynamic_prompt().filter(kition_cad_engine::pointmod::accepts)?;
        let point = self.aim.or(self.cursor).filter(|point| point.is_finite())?;
        let frame = Frame { base: prompt.base, angles: self.session.angle_settings() };
        let (x, y) = frame.values(Format::Cartesian, point);
        let (distance, angle) = frame.values(Format::Polar, point);
        let (lunits, luprec) = self.session.doc().map(|doc| (doc.header.i64("LUNITS", 2), doc.header.i64("LUPREC", 4))).unwrap_or((2, 4));
        let length = |value: f64| kition_cad_engine::units::format_distance(value, lunits, luprec);
        let degrees = (angle.to_degrees() * 10.0).round() / 10.0;
        let degrees = if degrees.is_finite() { degrees.rem_euclid(360.0) } else { 0.0 };
        let text = json!({
            "cartesian": [length(x), length(y)],
            "polar": [length(distance), format!("{degrees}°")],
            "relative": prompt.base.is_some(),
        });
        let (px, py) = self.placement().to_doc(point);
        Some(ToolOverlay::Label { x: px, y: py, text: text.to_string(), color: DYNAMIC })
    }
}

/// The catalogue entry of [`SET_ENTRY`].
pub(super) fn command() -> KitCommand {
    KitCommand {
        id: SET_ENTRY.into(),
        title: "Dynamic Input Entry".into(),
        params: Some("{text: string, secondLocked?: bool}".into()),
        mutates: false,
        enabled: true,
        ..Default::default()
    }
    .with_kit_schema()
}
