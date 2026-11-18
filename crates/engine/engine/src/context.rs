//! The editor's state besides the drawing, for an explicit call context (Kit Process Protocol
//! section 7a; task card C4/C5/C6-cad): what a command may read besides its parameters (the
//! current layout or Model space, the MSPACE viewport, the view of each space and its history,
//! the selection and the previous selection, a pending selection window), taken and put back
//! without an edit, and set for one call. None of it moves the revision, marks the drawing dirty
//! or joins an undo history.
//!
//! The current layer (`CLAYER`) and the other "current" properties new objects take (`CECOLOR`,
//! `CELTYPE`, …) are not here: they are the drawing's header variables, which the file saves.

use kition_cad_doc::{Handle, Space};

use crate::{EngineError, PendingWindow, Result, Session, View};

/// A snapshot of the editor's state ([`Session::editor_state`]).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EditorState {
    /// The tab shown: Model or a layout.
    pub space: Space,
    /// The MSPACE viewport of that layout, if model space is being edited through one.
    pub mspace: Option<Handle>,
    /// The view of each space, and the views ZOOM Previous goes back to.
    pub views: Vec<(Space, View)>,
    pub view_history: Vec<View>,
    pub selection: Vec<Handle>,
    pub previous_selection: Vec<Handle>,
    pub pending_window: Option<PendingWindow>,
}

impl Session {
    /// The editor's state now; `None` without a drawing.
    pub fn editor_state(&self) -> Option<EditorState> {
        let st = self.state().ok()?;
        Some(EditorState {
            space: st.space.clone(),
            mspace: st.mspace,
            views: st.views.clone(),
            view_history: st.view_history.clone(),
            selection: st.selection.clone(),
            previous_selection: st.previous_selection.clone(),
            pending_window: self.pending_window,
        })
    }

    /// Puts `state` back. What the drawing no longer has is dropped: a deleted layout makes Model
    /// current, a deleted viewport ends MSPACE, deleted entities leave the selections. No edit:
    /// the revision, the dirty state and the undo history stay as they are.
    pub fn restore_editor_state(&mut self, state: &EditorState) -> Result<()> {
        let pending = state.pending_window;
        let st = self.state_mut()?;
        let space = match &state.space {
            Space::Paper(name) if st.doc.layout(name).is_none() => Space::Model,
            other => other.clone(),
        };
        let alive = |h: &Handle| st.doc.entity(*h).is_some();
        let mspace = state.mspace.filter(|h| st.doc.space(&space).is_some_and(|store| store.get(*h).is_some()));
        let selection = state.selection.iter().copied().filter(alive).collect();
        let previous = state.previous_selection.iter().copied().filter(alive).collect();
        st.space = space;
        st.mspace = mspace;
        st.views = state.views.clone();
        st.view_history = state.view_history.clone();
        st.selection = selection;
        st.previous_selection = previous;
        self.pending_window = pending;
        Ok(())
    }

    /// Makes `space` current for one call (its paper space for a layout: MSPACE ends) and
    /// `selection` (entities in it) the selection; `None` leaves each as it is. A change of space
    /// clears the selection, as switching tabs does. The caller checks the ids first. No edit.
    pub fn set_call_context(&mut self, space: Option<Space>, selection: Option<Vec<Handle>>) -> Result<()> {
        let st = self.state_mut()?;
        if let Some(space) = space {
            if let Space::Paper(name) = &space
                && st.doc.layout(name).is_none()
            {
                return Err(EngineError::Other(format!("no layout `{name}`")));
            }
            if st.space != space || st.mspace.is_some() {
                st.space = space;
                st.mspace = None;
                st.selection.clear();
            }
        }
        if let Some(selection) = selection {
            st.selection = selection;
        }
        self.pending_window = None;
        Ok(())
    }
}
