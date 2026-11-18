//! C8-cad-pin: round-tripped native views must not invent writes.
mod common;
use common::{drawing, new_replica, person};
use kdm_model::{read_document, write_changes};

#[test]
fn an_unchanged_rebuilt_view_does_not_write() {
    let (mut replica, _) = new_replica(&drawing());
    let (before, mut ids) = read_document(&replica).unwrap();
    let mut after = before.clone();
    after.model = rebuilt(&before.model);
    for block in after.blocks.values_mut() {
        let mut copy = block.as_ref().clone();
        copy.entities = rebuilt(&block.entities);
        *block = std::sync::Arc::new(copy);
    }
    for layout in &mut after.layouts {
        layout.entities = rebuilt(&layout.entities);
    }
    let version = replica.version();
    let state = replica.state_json().unwrap();
    let wrote = replica.transact(&person("pin"), |tx| write_changes(tx, &before, &after, &mut ids)).unwrap();
    assert!(!wrote, "the rebuilt view reported a phantom write");
    assert_eq!(replica.state_json().unwrap(), state);
    assert_eq!(replica.version(), version);
}

fn rebuilt(store: &kition_cad_doc::EntityStore) -> kition_cad_doc::EntityStore {
    let mut out = kition_cad_doc::EntityStore::new();
    for entity in store.iter() {
        out.push(entity.as_ref().clone());
    }
    out
}
