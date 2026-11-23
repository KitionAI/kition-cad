//! `CadSession`: the engine bridge's replica: `Send`, record/undo/redo, the exchange with a peer,
//! `holds`, and the reachable undo steps.

mod common;

use common::{canonical, drawing, line, line_a, move_entity, on, person};
use kdm_model::CadSession;
use kition_cad_doc::{Drawing, Space};
use kition_kit::kdm::Origin;

fn send(from: &CadSession, to: &mut CadSession) {
    let since = to.version();
    to.import_changes(&from.export_changes(&since).unwrap()).unwrap();
}

fn edited(s: &CadSession, f: impl FnOnce(&mut Drawing)) -> Drawing {
    let mut d = s.drawing().clone();
    f(&mut d);
    d
}

#[test]
fn a_session_is_send() {
    fn send_able<T: Send>() {}
    send_able::<CadSession>();
}

#[test]
fn record_undo_redo_and_a_peer_agree() {
    let mut a = CadSession::create(&drawing()).unwrap();
    let mut b = CadSession::open_snapshot(&a.snapshot().unwrap()).unwrap();
    assert_eq!(canonical(a.drawing()), canonical(b.drawing()));

    let h = line_a(a.drawing());
    let after = edited(&a, |d| move_entity(d, h, 5.0, 0.0));
    assert_eq!(a.record(&person("a"), &after).unwrap(), 1, "one undo step");
    assert_eq!(a.record(&person("a"), &after).unwrap(), 0, "nothing changed, no step");
    send(&a, &mut b);
    assert_eq!(canonical(a.drawing()), canonical(b.drawing()));
    assert_eq!(a.state_hash().unwrap(), b.state_hash().unwrap());

    assert!(a.undo().unwrap().0);
    assert_ne!(canonical(a.drawing()), canonical(b.drawing()));
    assert!(a.redo().unwrap().0);
    assert_eq!(canonical(a.drawing()), canonical(b.drawing()));
    assert!(!a.redo().unwrap().0, "nothing more to redo");
}

#[test]
fn a_system_record_is_no_undo_step() {
    let mut a = CadSession::create(&drawing()).unwrap();
    let after = edited(&a, |d| d.header.set_f64("LTSCALE", 4.0));
    assert_eq!(a.record(&Origin::system("view"), &after).unwrap(), 0);
    assert!(!a.can_undo());
}

#[test]
fn new_entities_take_handles_from_the_sessions_range() {
    let mut a = CadSession::create(&drawing()).unwrap();
    let (start, end) = a.handle_range();
    let mut d = a.drawing().clone();
    d.handseed = a.next_handle(&d);
    let h = d.add(&Space::Model, on("0"), line((0.0, 0.0), (1.0, 1.0))).unwrap();
    assert!((start..end).contains(&h.0));
    a.record(&person("a"), &d).unwrap();
    assert_eq!(a.next_handle(a.drawing()), h.0 + 1);
    assert!(a.entity_object(h).is_some());
}

#[test]
fn holds_compares_content_not_ids() {
    let a = CadSession::create(&drawing()).unwrap();
    assert!(a.holds(&drawing()).unwrap());
    let mut other = drawing();
    other.header.set_f64("LTSCALE", 5.0);
    assert!(!a.holds(&other).unwrap());
    let mut current = drawing();
    current.header.set_str("CLAYER", "Walls");
    assert!(!a.holds(&current).unwrap());
}

#[test]
fn a_file_read_again_pairs_its_entities_by_handle() {
    let d = drawing();
    let mut a = CadSession::create(&d).unwrap();
    let h = line_a(&d);
    let object = a.entity_object(h).unwrap();
    let mut file = d.clone();
    move_entity(&mut file, h, 5.0, 0.0);
    a.reread(&d, &file).unwrap();
    assert_eq!(a.entity_object(h), Some(object), "the moved line keeps its object");
    assert!(a.holds(&file).unwrap());
}

#[test]
fn retained_steps_bound_undo() {
    let mut a = CadSession::create(&drawing()).unwrap();
    let h = line_a(a.drawing());
    for i in 0..3 {
        let after = edited(&a, |d| move_entity(d, h, 1.0 + f64::from(i), 0.0));
        a.record(&person("a"), &after).unwrap();
    }
    assert_eq!(a.steps(), (3, 0));
    a.retain_steps(1, 0);
    assert!(a.undo().unwrap().0);
    assert!(!a.can_undo(), "only the retained step could be undone");
}
