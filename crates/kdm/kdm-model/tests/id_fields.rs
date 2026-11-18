//! Every handle stored as data must be written as an object id and translated back
//! (`kdm_model`'s `refs` module). This lists every model field typed with a handle, so a new one
//! fails here until the bridge handles it.

const SOURCES: &[(&str, &str)] = &[
    ("lib.rs", include_str!("../../../engine/doc/src/lib.rs")),
    ("entity.rs", include_str!("../../../engine/doc/src/entity.rs")),
    ("tables.rs", include_str!("../../../engine/doc/src/tables.rs")),
    ("constraint.rs", include_str!("../../../engine/doc/src/constraint.rs")),
    ("header.rs", include_str!("../../../engine/doc/src/header.rs")),
    ("plotstyle.rs", include_str!("../../../engine/doc/src/plotstyle.rs")),
    ("store.rs", include_str!("../../../engine/doc/src/store.rs")),
    ("extents.rs", include_str!("../../../engine/doc/src/extents.rs")),
    ("library.rs", include_str!("../../../engine/doc/src/library.rs")),
];

/// `file: field` for every field whose type names a handle, and how the bridge handles it.
const HANDLED: &[&str] = &[
    // Entity.handle: the entity's own identity, its `handle` register (replicated, as DXF keeps
    // it); also DimAssoc.handle: refs::kind_out/kind_in, the dimension's `assoc[].handle`.
    "entity.rs: handle: Handle",
    // AssocSnap::Intersection { other }: refs::kind_out/kind_in, `assoc[].snap.intersection.other`.
    "entity.rs: other: Handle",
    // GeomRef.handle: refs::each_ref_handle, a constraint's `refs[].handle`.
    "constraint.rs: handle: Handle",
    // ParametricSettings: refs::each_parametric_list.
    "constraint.rs: bar_exceptions: Vec<Handle>",
    "constraint.rs: dim_exceptions: Vec<Handle>",
    // Group.members: refs::handles_out/handles_in on a group record's `members`.
    "tables.rs: members: Vec<crate::Handle>",
    // The store's handle index: derived from the entities, never written.
    "store.rs: shards: Vec<Arc<HashMap<Handle, u32>>>",
];

#[test]
fn every_handle_field_is_handled() {
    let mut found = Vec::new();
    for (file, source) in SOURCES {
        for line in source.lines() {
            let line = line.trim();
            if line.starts_with("//")
                || line.starts_with("pub fn")
                || line.starts_with("pub(crate) fn")
                || line.starts_with("fn ")
                || line.contains("=>")
                || line.contains(" = ")
            {
                continue;
            }
            let line = line.trim_start_matches("pub ").trim_end_matches(',');
            if line.contains("Handle") && line.contains(": ") && !line.starts_with("use ") && !line.contains("Handle(") {
                found.push(format!("{file}: {line}"));
            }
        }
    }
    found.sort();
    found.dedup();
    let unhandled: Vec<_> = found.iter().filter(|f| !HANDLED.contains(&f.as_str())).collect();
    assert!(unhandled.is_empty(), "model fields holding handles that kdm-model does not translate: {unhandled:#?}");
    let stale: Vec<_> = HANDLED.iter().filter(|h| !found.iter().any(|f| f == *h)).collect();
    assert!(stale.is_empty(), "listed as handled but no longer in the model (update this list): {stale:#?}");
}
