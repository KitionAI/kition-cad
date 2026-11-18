//! Round trips: a drawing written to a replica reads back with the same content (ids aside), for
//! the repository's DXF samples, a drawing exercising every replicated part, and (when
//! `KITION_CAD_CORPUS=<dir>` is set) up to 40 `.dxf` files of a local corpus.

mod common;

use common::{assert_same, drawing, new_replica, person};
use kdm_model::{CadModel, decode, read_document, write_changes};
use kition_cad_doc::{Drawing, EntityKind, HVal};
use kition_kit::kdm::ActorId;
use kition_kit::session::{KitModel, KitSession, MemoryBlobs};

/// `d` through `import`'s path (written in a creating transaction) and back. Also checks that
/// diffing the read view against a deep copy of itself (no chunk or block shared, so every
/// entity is compared) writes nothing.
fn round_trip(d: &Drawing) -> Drawing {
    let (mut replica, _) = new_replica(d);
    let (read, mut ids) = read_document(&replica).unwrap_or_else(|error| panic!("read: {error}"));
    let mut copy = read.clone();
    copy.model = rebuilt(&read.model);
    for layout in &mut copy.layouts {
        layout.entities = rebuilt(&layout.entities);
    }
    for block in copy.blocks.values_mut() {
        let mut b = (**block).clone();
        b.entities = rebuilt(&b.entities);
        *block = std::sync::Arc::new(b);
    }
    let wrote =
        replica.transact(&person("t"), |tx| write_changes(tx, &read, &copy, &mut ids)).unwrap_or_else(|error| panic!("write_changes: {error}"));
    assert!(!wrote, "an unchanged view wrote changes");
    read
}

/// A store with the same entities in new chunks.
fn rebuilt(s: &kition_cad_doc::EntityStore) -> kition_cad_doc::EntityStore {
    let mut out = kition_cad_doc::EntityStore::new();
    for e in s.iter() {
        out.push((**e).clone());
    }
    out
}

fn sample(path: &str) -> Drawing {
    let bytes = std::fs::read(format!("{}/../../../{path}", env!("CARGO_MANIFEST_DIR"))).unwrap_or_else(|error| panic!("{path}: {error}"));
    decode(path, &bytes).unwrap_or_else(|error| panic!("decode {path}: {error}"))
}

#[test]
fn repository_samples_round_trip() {
    for path in ["examples/apartment.dxf", "web/public/sample/Sample.dxf"] {
        let original = sample(path);
        assert!(original.entity_count() > 0, "{path} has entities");
        assert_same(path, &original, &round_trip(&original));
    }
}

#[test]
fn a_drawing_with_every_part_round_trips() {
    let d = drawing();
    let read = round_trip(&d);
    assert_same("drawing", &d, &read);
    // The header keeps each variable's type (a whole real stays a real).
    assert_eq!(read.header.get("LTSCALE"), Some(&HVal::Real(2.0)));
    assert_eq!(read.header.get("ORTHOMODE"), Some(&HVal::Int(1)));
    // Handles survive as they were, and references by handle with them.
    let handles = |d: &Drawing| d.model.iter().map(|e| e.handle).collect::<Vec<_>>();
    assert_eq!(handles(&d), handles(&read));
    let dim = read.model.iter().find_map(|e| if let EntityKind::Dimension(dim) = &e.kind { Some(dim.clone()) } else { None });
    assert_eq!(dim.map(|d| d.assoc.len()), Some(2), "both associations are kept");
    assert_eq!(read.groups[0].members.len(), 2);
    assert_eq!(read.constraints.len(), 1);
}

#[test]
fn a_dangling_reference_in_a_file_is_kept_as_the_handle_it_was() {
    let mut d = drawing();
    d.groups[0].members.push(kition_cad_doc::Handle(0xDEAD));
    d.constraints[0].refs.push(kition_cad_doc::GeomRef::whole(kition_cad_doc::Handle(0xBEEF)));
    let read = round_trip(&d);
    assert_same("dangling", &d, &read);
}

#[test]
fn names_differing_only_in_case_both_survive() {
    let mut d = drawing();
    d.layers.push(kition_cad_doc::Layer::new("walls"));
    d.layers.push(kition_cad_doc::Layer::new("Walls"));
    assert_same("case", &d, &round_trip(&d));
}

#[test]
fn a_drawing_with_an_undefined_number_is_refused() {
    let mut d = drawing();
    let h = common::the_circle(&d);
    d.model.modify(h, |e| {
        if let EntityKind::Circle(c) = &mut e.kind {
            c.radius = f64::NAN;
        }
    });
    let mut ids = kdm_model::Ids::default();
    let made = kition_kit::kdm::Replica::create_with(
        CadModel::schema(),
        kition_kit::kdm::DocId::new(),
        common::actor(1),
        &kition_kit::kdm::Origin::system("create"),
        |tx| kdm_model::write_document(tx, &d, &mut ids).map(|_| ()),
    );
    assert!(made.is_err(), "NaN cannot round-trip through JSON");
}

#[test]
fn import_and_export_through_a_session() {
    let d = drawing();
    let bytes = kition_cad_io::write(&d, "Plan.dxf").unwrap_or_else(|error| panic!("write: {error}"));
    let session: KitSession<CadModel> =
        KitSession::import("Plan.dxf", &bytes, Box::new(MemoryBlobs::default()), ActorId::from_u64(7).unwrap_or_else(|_| panic!("actor")))
            .unwrap_or_else(|error| panic!("import: {error}"));
    let exported = session.export("Plan.dxf").unwrap_or_else(|error| panic!("export: {error}"));
    let again = decode("Plan.dxf", &exported).unwrap_or_else(|error| panic!("decode: {error}"));
    let original = decode("Plan.dxf", &bytes).unwrap_or_else(|error| panic!("decode: {error}"));
    assert_eq!(again.entity_count(), original.entity_count());
    assert_eq!(again.layers.len(), original.layers.len());
}

#[test]
fn schema_parses() {
    let schema = CadModel::schema();
    assert_eq!(schema.kit, "kit.cad");
}

#[test]
fn new_handles_come_from_the_replicas_own_range() {
    let (replica, _) = new_replica(&drawing());
    let (read, _) = read_document(&replica).unwrap_or_else(|error| panic!("read: {error}"));
    let (start, end) = kdm_model::handle_range(replica.actor());
    assert!((start..end).contains(&read.handseed), "{:X} in {start:X}..{end:X}", read.handseed);
    let other = kdm_model::handle_range(common::actor(2));
    assert!(other.1 <= start || other.0 >= end, "two actors' ranges do not overlap");
}

/// `KITION_CAD_CORPUS=<dir>`: up to 40 `.dxf` files there, each decoded, written, read back and
/// compared. Skipped when unset.
#[test]
fn dxf_corpus_round_trips() {
    let Ok(dir) = std::env::var("KITION_CAD_CORPUS") else { return };
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|error| panic!("{dir}: {error}"))
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("dxf")))
        .collect();
    files.sort();
    files.truncate(40);
    let mut checked = 0;
    for path in &files {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let bytes = std::fs::read(path).unwrap_or_else(|error| panic!("{name}: {error}"));
        let Ok(original) = decode(&name, &bytes) else {
            eprintln!("{name}: not readable, skipped");
            continue;
        };
        if kdm_model::CadSession::create(&original).is_err() {
            eprintln!("{name}: holds numbers a replica cannot keep, skipped");
            continue;
        }
        assert_same(&name, &original, &round_trip(&original));
        checked += 1;
    }
    eprintln!("corpus: {checked} of {} files round-tripped", files.len());
}
