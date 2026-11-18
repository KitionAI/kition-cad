//! C8-cad-pin: cleanup changes only group membership, across three concurrent writers.
mod common;
use common::*;

#[test]
fn member_cleanup_keeps_independent_description_and_selectability_edits() {
    for reverse in [false, true] {
        let mut a = first(&drawing());
        let mut b = peer(&a, 2);
        let mut c = peer(&a, 3);
        let removed = line_a(&a.model().drawing);
        edit(&mut a, |d| {
            d.model.remove(removed);
        });
        edit(&mut b, |d| {
            d.groups[0].description = "edited".into();
        });
        edit(&mut c, |d| {
            d.groups[0].selectable = false;
        });
        let mut replicas = if reverse { vec![c, b, a] } else { vec![a, b, c] };
        sync_all(&mut replicas);
        for replica in &replicas {
            let group = &replica.model().drawing.groups[0];
            assert_eq!(group.description, "edited");
            assert!(!group.selectable);
            assert!(!group.members.contains(&removed));
            assert_eq!(group.members.len(), 1);
            assert_replica_clean(replica.replica());
        }
    }
}
