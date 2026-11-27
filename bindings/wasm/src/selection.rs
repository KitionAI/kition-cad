//! Selecting in the editor beyond picks and windows: Select Similar (the drawing menu's, as CAD's
//! SELECTSIMILAR does by type and layer).
use super::*;

/// The editor command that selects the objects like the selected ones.
pub const SIMILAR: &str = "select.similar";

impl CadDocument {
    /// [`SIMILAR`]: selects every object shown in the space being edited that has the type and the
    /// layer of one of the selected objects (a line on Walls selects the lines on Walls). It changes
    /// only the selection; it answers how many objects are selected and their handles.
    pub(super) fn select_similar(&mut self) -> Result<Value, String> {
        let selection = self.session.selection();
        if selection.is_empty() {
            return Err("Select an object first: Select Similar selects the objects like it.".into());
        }
        let state = self.session.state().map_err(engine_error)?;
        let doc = &state.doc;
        let mut kinds: Vec<(&str, String)> = Vec::new();
        for handle in &selection {
            let Some(entity) = doc.entity(*handle) else { continue };
            let key = (entity.kind.type_name(), entity.common.layer.to_ascii_lowercase());
            if !kinds.contains(&key) {
                kinds.push(key);
            }
        }
        let store = doc.space(&state.edit_space()).ok_or("no such space")?;
        let similar: Vec<kition_cad_doc::Handle> = store
            .iter()
            .filter(|e| {
                doc.is_visible(e) && kinds.iter().any(|(kind, layer)| e.kind.type_name() == *kind && e.common.layer.eq_ignore_ascii_case(layer))
            })
            .map(|e| e.handle)
            .collect();
        let handles: Vec<String> = similar.iter().map(|handle| handle.hex()).collect();
        self.session.set_selection(similar);
        Ok(json!({ "selected": handles.len(), "handles": handles }))
    }
}

/// The catalogue entry of [`SIMILAR`].
pub(super) fn command() -> KitCommand {
    KitCommand { id: SIMILAR.into(), title: "Select Similar".into(), params: Some("{}".into()), mutates: false, enabled: true, ..Default::default() }
        .with_kit_schema()
}
