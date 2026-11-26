//! Editing in place (walkthrough N12): a double-click between commands on a text, an attribute
//! definition, a paragraph (MTEXT) or a dimension asks the editor to open a box on the drawing
//! where the text is, and on a hatch to open its pattern, scale and angle. The request is in
//! `inspect()` under `/drafting/edit`, inside the editor state ([`crate::EDITOR_STATE`]), with a
//! serial the editor follows; the editor writes the new text through `text.edit` (one undo
//! step) and the hatch through `hatch.edit`.
//!
//! TEXT and MTEXT ask for their text where it goes: the prompt's text box (the engine's
//! `Prompt::text_box`) is under `/drafting/commandLine/prompt/textBox`.
use kition_cad_doc::{Entity, EntityKind};

use super::*;

/// The text box of a prompt or an object as the editor places it: in drawing units, the start
/// (`at`: a line's left end on its baseline, a paragraph's top-left corner, a dimension text's
/// middle when `centered`), the height, the rotation in degrees and a paragraph's width.
pub(super) fn text_box(text_box: &kition_cad_engine::TextBox) -> Value {
    json!({
        "at": [finite(text_box.at.x), finite(text_box.at.y)],
        "height": finite(text_box.height),
        "rotation": finite(text_box.rotation.to_degrees()),
        "width": finite(text_box.width),
        "paragraphs": text_box.paragraphs,
    })
}

fn finite(v: f64) -> f64 {
    if v.is_finite() { v } else { 0.0 }
}

/// What a double-click on `e` edits in place, if anything: the text and where its box goes, or a
/// hatch's settings.
pub(super) fn request(doc: &kition_cad_doc::Drawing, e: &Entity) -> Option<Value> {
    let bounds = kition_cad_doc::entity_bounds(doc, e, 0);
    let corner = |top: bool| if bounds.is_empty() { None } else { Some(Vec2::new(bounds.min.x, if top { bounds.max.y } else { bounds.min.y })) };
    let (kind, text, place) = match &e.kind {
        EntityKind::Text(t) | EntityKind::AttDef(kition_cad_doc::Attrib { text: t, .. }) => {
            // Unturned text: its box over the text as it is drawn; turned: at its start.
            let at = if t.rotation.abs() < 1e-9 { corner(false).unwrap_or(t.insert.xy()) } else { t.insert.xy() };
            let place = kition_cad_engine::TextBox { at, height: t.height, rotation: t.rotation, width: 0.0, paragraphs: false };
            ("text", t.value.clone(), text_box(&place))
        }
        EntityKind::MText(t) => {
            let at = if t.rotation.abs() < 1e-9 { corner(true).unwrap_or(t.insert.xy()) } else { t.insert.xy() };
            let place = kition_cad_engine::TextBox { at, height: t.height, rotation: t.rotation, width: t.width, paragraphs: true };
            ("paragraphs", t.contents.replace("\\P", "\n"), text_box(&place))
        }
        EntityKind::Dimension(d) => {
            // Where the text is drawn, as the renderer places it.
            let geometry = kition_cad_render::dimension_in(doc, d);
            let height = if geometry.text_height > 0.0 { geometry.text_height } else { 2.5 };
            let place = kition_cad_engine::TextBox { at: geometry.text_pos, height, rotation: geometry.text_angle, width: 0.0, paragraphs: false };
            let mut place = text_box(&place);
            place["centered"] = json!(true);
            // An empty override is the measurement: "<>" stands for it, as TEXTEDIT shows it.
            ("dimension", if d.text.is_empty() { "<>".to_owned() } else { d.text.clone() }, place)
        }
        EntityKind::Hatch(h) => {
            return Some(json!({
                "kind": "hatch",
                "handle": e.handle.hex(),
                "pattern": h.pattern,
                "scale": finite(h.scale),
                "angle": finite(h.angle.to_degrees()),
            }));
        }
        _ => return None,
    };
    Some(json!({ "kind": kind, "handle": e.handle.hex(), "text": text, "box": place }))
}

impl CadDocument {
    /// A double-click at `at`: between commands, the object under the pick box opens in place
    /// (text) or in its dialog (hatch); a grip the double-click's second click made hot goes
    /// cold, as the reference does for a hatch. Objects on locked layers are not picked.
    pub(super) fn double_click(&mut self, at: Vec2) {
        if self.session.running.is_some() {
            return;
        }
        let Some(handle) = self.session.hover(at, self.session.pick_aperture()) else { return };
        let Ok(doc) = self.session.doc() else { return };
        let Some(found) = doc.entity(handle).and_then(|e| request(doc, e)) else { return };
        self.hot_grip = None;
        self.marked.clear();
        self.forget_aim();
        self.session.set_selection(vec![handle]);
        self.edit_serial = self.edit_serial.saturating_add(1);
        let mut found = found;
        found["serial"] = json!(self.edit_serial);
        self.edit_request = Some(found);
    }
}
