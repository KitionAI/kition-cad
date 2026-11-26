//! The command bridge to each drawing's KDM replica (Kition phase 3g), the same design as Kition
//! Slide's, Film's and Photo's.
//!
//! A drawing's replica ([`kdm_model::CadSession`]) is its shared source of truth; [`DocState::doc`]
//! is the engine's own view of it. Commands and interactive commands run exactly as before on the
//! in-memory [`Drawing`]; at the end of every top-level call ([`Session::execute`],
//! [`Session::start`], [`Session::input`], [`Session::cmdline`], [`Session::script`],
//! [`Session::cancel`], [`Session::undo`], [`Session::redo`], and [`Session::sync_kdm`] for
//! hosts that change the drawing directly, such as a view inside a layout viewport) the bridge
//! writes what changed since the replica last saw it ([`Link::synced`]) as one transaction per
//! new history step, found by [`DocState::pushed`] (a counter, not the stack's length: at the
//! limit a push drops the oldest step). So nested commands, the post-command passes (groups
//! pruned, constraints re-solved, associative dimensions updated) and the panic guard's rollback
//! are all replicated, each in the transaction of the step they belong to. A running interactive
//! command is written when it ends, as the one step it is (an import in the middle of it writes
//! it so far first; the step then undoes every transaction it took).
//!
//! Undo is per person: each history step remembers how many replica undo steps it took
//! ([`crate::Snapshot::steps`]); undo and redo step the replica that many times and take the
//! replica's result (with every collaborator's edit since) as the drawing, not the engine's own
//! snapshot. The selection, the previous selection, the space and MSPACE viewport, views and view
//! history, the cursor, tracking, the running command and the clipboard are this person's own:
//! never written; after an import the selection drops what vanished and the space follows a
//! renamed layout ([`DocState::follow_kdm`]).
//!
//! New entities take handles from this replica's own range ([`kdm_model::handle_range`]):
//! [`DocState::fit_handseed`] moves the drawing's next handle into it, so two people drawing at
//! once never mint the same handle.
//!
//! A replica that refuses a write or an undo is left ([`Session::kdm_resets`]): the drawing keeps
//! working from its own history snapshots, and the next edit makes a fresh replica.

use std::sync::Arc;

use kdm_model::CadSession;
use kdm_model::kition_kit::kdm::{Origin, UndoScope, Version};
use kdm_model::kition_kit::session::Damage;
use kition_cad_doc::{Drawing, Space};

use crate::{DocState, EngineError, Result, Session, View};

/// The origin of the local person's commands (used when nothing else was set with
/// [`Session::execute_as`]/[`Session::set_origin`]).
pub fn local_person() -> Origin {
    Origin::person("person:local")
}

fn kdm_error(error: impl std::fmt::Display) -> EngineError {
    EngineError::Other(format!("drawing replica: {error}"))
}

fn no_replica() -> EngineError {
    EngineError::Other("the drawing has no replica and none could be made".into())
}

/// One drawing's bridge state.
#[derive(Debug)]
pub struct Link {
    /// The replica, made lazily (the first edit or passthrough); `None` again after a reset.
    pub(crate) kdm: Option<CadSession>,
    /// The drawing as last written to (or read from) the replica.
    pub(crate) synced: Arc<Drawing>,
    pub(crate) resets: u64,
    /// [`DocState::pushed`] when the bridge last looked.
    pub(crate) mark: u64,
    /// Replica steps written for a running command before it ended (an import in the middle of
    /// it): they belong to the step it pushes.
    pub(crate) pending: u32,
    /// Replica steps of a running command written before another origin's command ran in the
    /// middle of it ([`Session::hold_running_edits`]): kept from that command's step, they go back
    /// to `pending` when it returns.
    pub(crate) held: u32,
    /// The drawing could not be held in a replica (it holds a number that is not finite, or a
    /// handle twice): edits do not try again until a host asks for the replica.
    pub(crate) refused: bool,
}

impl Link {
    pub(crate) fn new(doc: &Arc<Drawing>) -> Self {
        Self { kdm: None, synced: doc.clone(), resets: 0, mark: 0, pending: 0, held: 0, refused: false }
    }
}

/// The replica's view as the drawing, with a next handle never below `floor` when `floor` is in
/// the replica's own range, and `current`'s current properties (the current layer, styles…:
/// each person's own, never replicated, [`kdm_model::CURRENT_VARS`]).
fn view_of(kdm: &CadSession, floor: u64, current: &Drawing) -> Arc<Drawing> {
    let mut d = kdm.drawing().clone();
    kdm_model::carry_current(current, &mut d);
    let (start, end) = kdm.handle_range();
    if (start..end).contains(&floor) {
        d.handseed = d.handseed.max(floor);
    }
    Arc::new(d)
}

impl DocState {
    /// The drawing's replica, if it has made one.
    pub fn kdm(&self) -> Option<&CadSession> {
        self.link.kdm.as_ref()
    }

    /// How many times the drawing left its replica (a write or undo the replica refused).
    pub fn kdm_resets(&self) -> u64 {
        self.link.resets
    }

    /// Writes what the drawing changed since the replica last saw it: one transaction per
    /// history step pushed since the last look; a change that pushed none belongs to the top step,
    /// and is a `system` one (no undo step) when there is no step for it to belong to (a file's
    /// caches dropped as it opens) or right after an undo (so the redo steps survive). A running
    /// command (`live`) is left for its end unless `force` (an import must not lose it).
    pub(crate) fn sync_kdm(&mut self, origin: &Origin, force: bool, live: bool) {
        let pushed = self.pushed_since(self.link.mark);
        self.link.mark = self.pushed;
        if pushed.is_empty() && self.link.pending == 0 && Arc::ptr_eq(&self.link.synced, &self.doc) {
            return;
        }
        if pushed.is_empty() {
            if live {
                if force && !Arc::ptr_eq(&self.link.synced, &self.doc) {
                    let after = self.doc.clone();
                    let steps = self.record_kdm(origin, &after).unwrap_or(0);
                    self.link.pending = self.link.pending.saturating_add(steps);
                }
                return;
            }
            let mut steps = std::mem::take(&mut self.link.pending);
            if !Arc::ptr_eq(&self.link.synced, &self.doc) {
                // Nothing the history holds can take this back: the replica must not offer it as a
                // step of the person's either.
                let origin = if self.redo.is_empty() && !self.undo.is_empty() { origin.clone() } else { Origin::system("view") };
                let after = self.doc.clone();
                steps = steps.saturating_add(self.record_kdm(&origin, &after).unwrap_or(0));
            }
            if steps > 0
                && let Some(top) = self.undo.last_mut()
            {
                top.steps = top.steps.saturating_add(steps);
            }
        } else {
            let last = pushed.end.saturating_sub(1);
            for i in pushed {
                let after = self.undo.get(i + 1).map(|e| e.doc.clone()).unwrap_or_else(|| self.doc.clone());
                let mut steps = if i == last { std::mem::take(&mut self.link.pending) } else { 0 };
                if !Arc::ptr_eq(&self.link.synced, &after) {
                    match self.record_kdm(origin, &after) {
                        Some(n) => steps = steps.saturating_add(n),
                        None => break,
                    }
                }
                if let Some(e) = self.undo.get_mut(i) {
                    e.steps = steps;
                }
            }
        }
        self.fit_handseed();
        self.trim_kdm();
    }

    /// Writes `after` on top of the replica (made now, baselined on [`Link::synced`], if there is
    /// none) and answers the undo steps that took; `None` when the replica refused and was left.
    fn record_kdm(&mut self, origin: &Origin, after: &Arc<Drawing>) -> Option<u32> {
        if self.link.kdm.is_none() {
            if self.link.refused {
                self.link.synced = after.clone();
                return Some(0);
            }
            match CadSession::create(&self.link.synced) {
                Ok(kdm) => self.link.kdm = Some(kdm),
                Err(error) => {
                    log::warn!("the drawing can't be held in a replica: {error}");
                    self.link.refused = true;
                    self.link.synced = after.clone();
                    return Some(0);
                }
            }
        }
        let kdm = self.link.kdm.as_mut()?;
        match kdm.record(origin, after) {
            Ok(steps) => {
                self.link.synced = after.clone();
                Some(steps)
            }
            Err(error) => {
                log::warn!("the drawing replica refused an edit: {error}");
                self.detach_kdm();
                None
            }
        }
    }

    /// The replica, made now from the current drawing if there is none yet (anything not written
    /// yet is written first; a running command stays for its end unless `force`).
    pub(crate) fn ensure_kdm(&mut self, origin: &Origin, force: bool, live: bool) -> Option<&mut CadSession> {
        self.sync_kdm(origin, force, live);
        if self.link.kdm.is_none() {
            match CadSession::create(&self.doc) {
                Ok(kdm) => {
                    self.link.kdm = Some(kdm);
                    self.link.synced = self.doc.clone();
                    self.link.refused = false;
                    self.fit_handseed();
                }
                Err(error) => {
                    log::warn!("the drawing can't be held in a replica: {error}");
                    self.link.refused = true;
                    return None;
                }
            }
        }
        self.link.kdm.as_mut()
    }

    /// Moves the drawing's next handle into this replica's own range (when it is not there yet:
    /// a drawing just opened, or one an AUDIT moved past every handle in use). A drawing that
    /// was clean stays clean: the next handle is not part of its content.
    pub(crate) fn fit_handseed(&mut self) {
        let Some(kdm) = self.link.kdm.as_ref() else { return };
        let (start, end) = kdm.handle_range();
        if (start..end).contains(&self.doc.handseed) {
            return;
        }
        let next = kdm.next_handle(&self.doc);
        let clean = Arc::ptr_eq(&self.doc, &self.saved);
        let synced = Arc::ptr_eq(&self.doc, &self.link.synced);
        Arc::make_mut(&mut self.doc).handseed = next;
        if clean {
            self.saved = self.doc.clone();
        }
        if synced {
            self.link.synced = self.doc.clone();
        }
    }

    /// Leaves the replica: every step's replica steps were the dropped replica's, and a fresh one
    /// never holds them.
    pub(crate) fn detach_kdm(&mut self) {
        self.link.kdm = None;
        self.link.resets = self.link.resets.saturating_add(1);
        self.link.synced = self.doc.clone();
        self.link.pending = 0;
        self.link.held = 0;
        for e in self.undo.iter_mut().chain(self.redo.iter_mut()) {
            e.steps = 0;
        }
    }

    /// Lets the replica forget the steps the history can no longer take.
    pub(crate) fn trim_kdm(&mut self) {
        let sum = |v: &[crate::Snapshot]| v.iter().map(|e| e.steps as usize).sum::<usize>();
        let undo = sum(&self.undo).saturating_add(self.link.pending as usize).saturating_add(self.link.held as usize);
        let redo = sum(&self.redo);
        if let Some(kdm) = self.link.kdm.as_mut() {
            kdm.retain_steps(undo, redo);
        }
    }

    /// After undo or redo set the drawing to the step's own snapshot: steps the replica too and
    /// takes its view (with every collaborator's edit since) instead. `floor` is the drawing's
    /// next handle before, which the view never lowers. The step, already on the other stack,
    /// counts the replica steps the calls left there, and the steps below it whose replica steps
    /// went with it leave their stack ([`DocState::take_kdm_steps`]).
    pub(crate) fn step_kdm(&mut self, steps: u32, undo: bool, scope: UndoScope, floor: u64) {
        if steps > 0 && self.link.kdm.is_some() {
            let Some((made, beyond)) = self.take_kdm_steps(steps, undo, scope) else {
                self.detach_kdm();
                self.link.mark = self.pushed;
                return;
            };
            let to = if undo { &mut self.redo } else { &mut self.undo };
            if let Some(e) = to.iter_mut().rev().find(|e| e.scope == scope) {
                e.steps = made;
            }
            self.take_below(undo, scope, beyond);
        }
        if let Some(kdm) = self.link.kdm.as_ref() {
            // The step's own snapshot (the caller set it) holds the current properties it had.
            self.doc = view_of(kdm, floor, &self.doc);
        }
        self.link.synced = self.doc.clone();
        self.link.mark = self.pushed;
        self.trim_kdm();
    }

    /// A step under the other scope's later ones: the replica undoes (or redoes) the step's
    /// `steps` transactions of `scope` and the drawing becomes its view, with the other scope's
    /// later edits and every collaborator's. `None`, changing nothing, when the replica does not
    /// hold them. Answers what [`DocState::take_kdm_steps`] does: the caller moves the step to the
    /// other stack counting the replica steps made there, takes the steps below that went with it
    /// ([`DocState::take_below`]), then trims ([`DocState::trim_kdm`]).
    pub(crate) fn replay_kdm(&mut self, steps: u32, undo: bool, scope: UndoScope) -> Option<(u32, usize)> {
        let floor = self.doc.handseed;
        let kdm = self.link.kdm.as_ref()?;
        let (available, can) = if undo { (kdm.steps().0, kdm.can_undo_in(scope)) } else { (kdm.steps().1, kdm.can_redo_in(scope)) };
        if steps == 0 || !can || usize::try_from(steps).unwrap_or(usize::MAX) > available {
            return None;
        }
        let Some(taken) = self.take_kdm_steps(steps, undo, scope) else {
            self.detach_kdm();
            self.link.mark = self.pushed;
            return None;
        };
        if let Some(kdm) = self.link.kdm.as_ref() {
            self.doc = view_of(kdm, floor, &self.doc);
        }
        self.link.synced = self.doc.clone();
        self.link.mark = self.pushed;
        Some(taken)
    }

    /// Undoes (or redoes) a history step's `steps` replica steps of `scope`. The replica passes
    /// over a step with nothing left to change (say a collaborator erased the entity it moved)
    /// and goes on to the one below in the same call (`UndoOutcome`): so the calls may take fewer
    /// of this step's replica steps back than they take off the stack, or reach below it into the
    /// scope's next history step. Answers the replica steps the calls made on the other stack and
    /// how many they took off beyond the step's own; `None` when the replica refused (the caller
    /// leaves it). A replica with nothing left to take stops the calls.
    fn take_kdm_steps(&mut self, steps: u32, undo: bool, scope: UndoScope) -> Option<(u32, usize)> {
        let kdm = self.link.kdm.as_mut()?;
        let want = steps as usize;
        let (mut consumed, mut made) = (0usize, 0u32);
        while consumed < want {
            match if undo { kdm.undo_in(scope) } else { kdm.redo_in(scope) } {
                Ok((outcome, _)) if outcome.steps() == 0 => break,
                Ok((outcome, _)) => {
                    consumed = consumed.saturating_add(outcome.steps());
                    made = made.saturating_add(u32::from(outcome.is_done()));
                }
                Err(_) => return None,
            }
        }
        Some((made, consumed.saturating_sub(want)))
    }

    /// The history steps of `scope` below an undone (or redone) one whose `beyond` replica steps
    /// its calls took with it: they leave their stack (a step keeps the replica steps left of it).
    pub(crate) fn take_below(&mut self, undo: bool, scope: UndoScope, mut beyond: usize) {
        let from = if undo { &mut self.undo } else { &mut self.redo };
        while beyond > 0 {
            let Some(i) = from.iter().rposition(|e| e.scope == scope && e.steps > 0) else { break };
            let below = from[i].steps as usize;
            if below <= beyond {
                from.remove(i);
                beyond -= below;
            } else {
                from[i].steps -= u32::try_from(beyond).unwrap_or(u32::MAX);
                beyond = 0;
            }
        }
    }

    /// After an import: the drawing follows the replica, the selections drop what vanished, the
    /// space follows a renamed layout (or goes back to Model), and an MSPACE viewport that is gone
    /// is left.
    pub(crate) fn follow_kdm(&mut self) {
        let Some(kdm) = self.link.kdm.as_ref() else { return };
        let before = self.doc.clone();
        self.doc = view_of(kdm, before.handseed, &before);
        self.link.synced = self.doc.clone();
        let doc = self.doc.clone();
        self.selection.retain(|h| doc.entity(*h).is_some());
        self.previous_selection.retain(|h| doc.entity(*h).is_some());
        self.follow_layout(&before);
        if let Some(h) = self.mspace
            && self.doc.space(&self.space).and_then(|s| s.get(h)).is_none()
        {
            self.mspace = None;
        }
        self.revision += 1;
    }
}

impl Session {
    /// Puts `from`'s current properties (the current layer, styles…: each person's own, never
    /// replicated, [`kdm_model::CURRENT_VARS`]) into the active drawing, as part of its view: not
    /// an edit, so a clean drawing stays clean and nothing is written to the replica.
    pub fn carry_current(&mut self, from: &Drawing) -> Result<()> {
        let st = self.state_mut()?;
        let clean = Arc::ptr_eq(&st.doc, &st.saved);
        let synced = Arc::ptr_eq(&st.doc, &st.link.synced);
        let mut d = (*st.doc).clone();
        kdm_model::carry_current(from, &mut d);
        st.doc = Arc::new(d);
        if clean {
            st.saved = st.doc.clone();
        }
        if synced {
            st.link.synced = st.doc.clone();
        }
        Ok(())
    }

    /// Reconcile an outside save onto a clean drawing while keeping both scoped histories.
    pub fn reconcile_reread(&mut self, baseline: &Drawing, fresh: &Drawing) -> Result<()> {
        self.active_kdm(false)?.reread(baseline, fresh).map_err(kdm_error)?;
        let state = self.state_mut()?;
        state.follow_kdm();
        state.saved = state.doc.clone();
        Ok(())
    }

    /// A top-level call: what it changed is written to the replicas when it returns (nested
    /// calls, such as a command another command runs, are written with it).
    pub(crate) fn top<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        self.depth = self.depth.saturating_add(1);
        let r = f(self);
        self.depth = self.depth.saturating_sub(1);
        self.sync_kdm();
        r
    }

    /// Writes every drawing's changes to its replica (a running command's when it ends). The
    /// session's own calls do this when they return; a host that changes a drawing directly (a
    /// view, [`DocState::set_view`], inside a layout viewport) calls it after.
    pub fn sync_kdm(&mut self) {
        if self.depth != 0 {
            return;
        }
        let origin = self.origin();
        let (live, active) = (self.running.is_some(), self.active);
        for (i, st) in self.docs.iter_mut().enumerate() {
            st.sync_kdm(&origin, false, live && i == active);
        }
    }

    /// Who the running (or next) command acts for: the origin of [`Session::execute_as`], or the
    /// local person.
    pub fn origin(&self) -> Origin {
        self.origin.clone().unwrap_or_else(local_person)
    }

    /// Sets who the following commands act for (`None`: the local person); answers the previous
    /// setting, to put back.
    pub fn set_origin(&mut self, origin: Option<Origin>) -> Option<Origin> {
        std::mem::replace(&mut self.origin, origin)
    }

    /// Before a top-level command of another origin than the running interactive command's (an
    /// agent's command while the person is in the middle of LINE): writes the running command's
    /// edits so far to the replica as its origin's, and holds those replica steps away from the
    /// other command's step. Without this, the segments the person had drawn were written with the
    /// agent's command, as the agent's step: the person's Undo of the LINE left them, the agent's
    /// took them. Answers the drawing whose steps are held, for
    /// [`Session::give_back_running_edits`].
    pub(crate) fn hold_running_edits(&mut self) -> Option<u64> {
        if self.depth != 0 {
            return None;
        }
        let origin = self.origin();
        let running = self.running.as_ref().filter(|run| run.origin != origin)?.origin.clone();
        let st = self.state_mut().ok()?;
        st.sync_kdm(&running, true, true);
        let pending = std::mem::take(&mut st.link.pending);
        st.link.held = st.link.held.saturating_add(pending);
        Some(st.uid)
    }

    /// After the command [`Session::hold_running_edits`] ran before: the held replica steps are
    /// the running command's again, for the step it pushes when it ends.
    pub(crate) fn give_back_running_edits(&mut self, held: Option<u64>) {
        let Some(uid) = held else { return };
        if let Some(st) = self.docs.iter_mut().find(|d| d.uid == uid) {
            let held = std::mem::take(&mut st.link.held);
            st.link.pending = st.link.pending.saturating_add(held);
        }
    }

    /// [`Session::execute`] as `origin` (a person, an agent): the replica records the edit as
    /// theirs, and it undoes as this replica's own step.
    pub fn execute_as(&mut self, id: &str, params: &serde_json::Value, origin: &Origin) -> Result<serde_json::Value> {
        let previous = self.set_origin(Some(origin.clone()));
        let r = self.execute(id, params);
        self.set_origin(previous);
        r
    }

    /// The active drawing's replica, if it has made one (never makes one).
    pub fn kdm(&self) -> Option<&CadSession> {
        self.state().ok()?.kdm()
    }

    /// How many times the active drawing left its replica. Hosts watch this: the next edit's
    /// fresh replica is a new KDM document collaborators must join from its snapshot.
    pub fn kdm_resets(&self) -> u64 {
        self.state().map_or(0, DocState::kdm_resets)
    }

    /// The active drawing's KDM identity, `None` before it has made a replica.
    pub fn doc_id(&self) -> Option<String> {
        Some(self.kdm()?.doc_id().to_string())
    }

    fn active_kdm(&mut self, force: bool) -> Result<&mut CadSession> {
        let origin = self.origin();
        let live = self.running.is_some();
        self.state_mut()?.ensure_kdm(&origin, force, live).ok_or_else(no_replica)
    }

    /// The active drawing's replica version.
    pub fn version(&mut self) -> Result<Version> {
        Ok(self.active_kdm(false)?.version())
    }

    /// The active drawing's replica as a snapshot, for a peer ([`Session::open_snapshot`]).
    pub fn snapshot(&mut self) -> Result<Vec<u8>> {
        self.active_kdm(false)?.snapshot().map_err(kdm_error)
    }

    /// SHA-256 of the active drawing's replicated state: equal on replicas that converged.
    pub fn state_hash(&mut self) -> Result<String> {
        self.active_kdm(false)?.state_hash().map_err(kdm_error)
    }

    /// The active drawing's changes since `since` (`Version::empty()` for all of them).
    pub fn export_changes(&mut self, since: &Version) -> Result<Vec<u8>> {
        self.active_kdm(false)?.export_changes(since).map_err(kdm_error)
    }

    /// Imports a collaborator's (or stored) changes into the active drawing's replica. Local
    /// edits not written yet, a running command's so far included, are written first. The drawing
    /// and its selection follow the replica; one whose view could not be read from it leaves it
    /// (a reset) rather than diff against it later.
    pub fn import_changes(&mut self, bytes: &[u8]) -> Result<Damage> {
        let kdm = self.active_kdm(true)?;
        let before = kdm.version();
        let result = kdm.import_changes(bytes);
        let changed = kdm.version() != before;
        let follow = result.is_ok() || (changed && kdm.view_loaded());
        let st = self.state_mut()?;
        if follow {
            st.follow_kdm();
        } else if changed {
            st.detach_kdm();
        }
        st.trim_kdm();
        result.map_err(kdm_error)
    }

    /// Takes a draft's changes (made on a replica forked from this one, `mergeChanges`) into the
    /// active drawing as one undo step of the scope the replica put it in: the person's who
    /// accepts it, whoever made the draft (KitModel M12a, data model D9), so the person's Undo
    /// takes the whole draft back while an agent's does not. Local edits not written yet are
    /// written first, and the drawing and its selection follow the replica. Answers that scope
    /// and the damage. A refused merge changes nothing.
    pub fn merge_changes(&mut self, bytes: &[u8], origin: &Origin) -> Result<(Option<UndoScope>, Damage)> {
        self.take_in("Merge Draft", |kdm| kdm.merge_changes(bytes, origin))
    }

    /// Writes the state a snapshot holds (a checkpoint of this drawing, `replica/restore`) over the
    /// active drawing by object id, as one undo step of `origin`'s scope (the replica writes it as
    /// a transaction of `origin`), taken in as a merged draft is ([`Session::merge_changes`]).
    /// Nothing changes when the states are equal.
    pub fn restore_state(&mut self, snapshot: &[u8], origin: &Origin) -> Result<Damage> {
        self.take_in("Restore Version", |kdm| kdm.restore_state(snapshot, origin)).map(|(_, damage)| damage)
    }

    /// One transaction `write` makes on the active drawing's replica, taken into the drawing as a
    /// command's step of the scope the replica put it in: the drawing follows the replica (as
    /// after an import), and the history gets the step, its replica undo steps the transaction's,
    /// when it is one.
    ///
    /// A running command ends first, as starting another command ends it (what it drew stays, as
    /// its own step): the replica's history would otherwise hold the command's steps on both sides
    /// of this one, and the command's Undo would take this one back with it.
    fn take_in(
        &mut self,
        label: &str,
        write: impl FnOnce(&mut CadSession) -> (u32, kdm_model::kition_kit::session::Result<(Option<UndoScope>, Damage)>),
    ) -> Result<(Option<UndoScope>, Damage)> {
        if self.running.is_some() {
            self.cancel();
        }
        let kdm = self.active_kdm(true)?;
        let before_version = kdm.version();
        let (steps, result) = write(kdm);
        let changed = kdm.version() != before_version;
        let follow = changed && (result.is_ok() || kdm.view_loaded());
        let st = self.state_mut()?;
        let (before, selection) = (st.doc.clone(), st.selection.clone());
        if follow {
            st.follow_kdm();
        } else if changed {
            st.detach_kdm();
        }
        if follow
            && steps > 0
            && let Some(scope) = result.as_ref().ok().and_then(|(scope, _)| *scope)
        {
            st.push_undo(crate::Snapshot { label: label.to_owned(), doc: before, selection, steps, scope });
            // The step is written already: the bridge has nothing of it left to record.
            st.link.mark = st.pushed;
        }
        st.trim_kdm();
        result.map_err(kdm_error)
    }

    /// Opens another replica's snapshot as a new drawing, a replica of the same KDM document
    /// under a fresh actor, and makes it active: clean, with an empty history.
    pub fn open_snapshot(&mut self, bytes: &[u8]) -> Result<usize> {
        let kdm = CadSession::open_snapshot(bytes).map_err(kdm_error)?;
        self.untitled_counter += 1;
        let title = format!("Drawing{}", self.untitled_counter);
        let index = self.open_drawing(kdm.drawing().clone(), &title, None);
        let st = self.state_mut()?;
        st.link.kdm = Some(kdm);
        st.link.synced = st.doc.clone();
        st.fit_handseed();
        Ok(index)
    }

    /// [`Session::open_snapshot`] for a host that keeps a drawing's replica beside its saved file.
    /// The file is what the person saved: when `file` is not empty and the replica does not hold
    /// its content (id-blind, as DXF keeps it: [`CadSession::holds_file`]), the file wins, recorded onto the replica as one `import`
    /// transaction (not an undo step) under the replica's own identity, so collaborators get it as
    /// ordinary changes. The drawing opens clean either way.
    pub fn open_with_replica(&mut self, file: &[u8], snapshot: &[u8]) -> Result<usize> {
        let loaded = if file.is_empty() { None } else { Some(kdm_model::decode("", file).map_err(kdm_error)?) };
        let index = self.open_snapshot(snapshot)?;
        let st = self.state_mut()?;
        if let Some(mut drawing) = loaded {
            let kdm = st.link.kdm.as_mut().ok_or_else(no_replica)?;
            if kdm.holds_file(&drawing).map_err(kdm_error)? {
                // The replica holds the file's content; the current properties are the file's.
                let mut view = (*st.doc).clone();
                kdm_model::carry_current(&drawing, &mut view);
                st.doc = Arc::new(view);
                st.saved = st.doc.clone();
                st.link.synced = st.doc.clone();
            } else {
                drawing.handseed = kdm.next_handle(&drawing).max(st.doc.handseed);
                kdm.record(&Origin::system("import"), &drawing).map_err(kdm_error)?;
                st.doc = Arc::new(drawing);
                st.saved = st.doc.clone();
                st.link.synced = st.doc.clone();
                st.selection = Vec::new();
                let ext = st.doc.extents(&Space::Model);
                if !ext.is_empty() {
                    st.set_view_quiet(View { center: ext.center(), height: (ext.height().max(ext.width()) * 1.1).max(1e-6) });
                }
            }
        }
        Ok(index)
    }
}
