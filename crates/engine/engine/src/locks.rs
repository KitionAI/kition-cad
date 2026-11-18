//! Entity locks apply to the entire atomic command, including solver and association hooks.
use std::sync::Arc;

use kition_cad_doc::{Drawing, Handle};

use crate::{EngineError, Session};

impl Session {
    pub(crate) fn refuse_locked_changes(
        &mut self,
        uid: Option<u64>,
        before: &Arc<Drawing>,
        selection: &[Handle],
        command: &str,
    ) -> crate::Result<()> {
        if !before.layers.iter().any(|layer| layer.locked) {
            return Ok(());
        }
        let state = match uid {
            Some(uid) => self.docs.iter_mut().find(|d| d.uid == uid),
            None => self.state_mut().ok(),
        };
        let Some(state) = state.filter(|s| !Arc::ptr_eq(before, &s.doc)) else { return Ok(()) };
        let after = &state.doc;
        // EntityStore::modify makes a fresh entity Arc even for a constant
        // assignment of the same value. Such a write still requires an unlocked target.
        let locked = before
            .model
            .iter()
            .chain(before.layouts.iter().flat_map(|l| l.entities.iter()))
            .chain(before.blocks.values().flat_map(|b| b.entities.iter()))
            .filter(|e| before.layer(&e.common.layer).is_some_and(|l| l.locked))
            .find(|old| {
                let now = after.entity(old.handle).or_else(|| after.blocks.values().find_map(|b| b.entities.get(old.handle)));
                now.is_none_or(|now| !Arc::ptr_eq(now, *old)) || before.space_of(old.handle) != after.space_of(old.handle)
            })
            .map(|e| e.handle);
        let Some(handle) = locked else { return Ok(()) };
        state.doc = before.clone();
        state.selection = selection.to_vec();
        Err(EngineError::BadParams { cmd: command.into(), msg: format!("object {} is on a locked layer. Nothing was changed", handle.hex()) })
    }
}
