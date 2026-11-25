//! Two replicas through `KitSession<CadModel>` on one line of changes: undo, the table's cells as
//! objects, and each person's own settings, which stay theirs across the exchange.

mod common;

use common::{Session, assert_no_dangling, assert_replica_clean, canonical, drawing, edit, first, line, line_a, on, peer, sync};
use kition_cad_doc::{EntityKind, Space};

fn pair() -> (Session, Session) {
    let a = first(&drawing());
    let b = peer(&a, 2);
    (a, b)
}

fn assert_converged(a: &Session, b: &Session) {
    assert_eq!(a.state_hash().ok(), b.state_hash().ok(), "same replica state");
    assert_eq!(canonical(&a.model().drawing), canonical(&b.model().drawing), "same view");
    assert_no_dangling(&a.model().drawing);
    assert_replica_clean(a.replica());
}

#[test]
fn undo_brings_back_a_deleted_entity_with_its_handle_and_constraint() {
    let (mut a, _b) = pair();
    let la = line_a(&a.model().drawing);
    let id = a.model().drawing.constraints[0].id;
    edit(&mut a, |d| {
        d.model.remove(la);
        d.constraints.clear();
    });
    let (undone, _) = a.undo().unwrap_or_else(|error| panic!("undo: {error}"));
    assert!(undone);
    let d = &a.model().drawing;
    assert!(d.entity(la).is_some(), "the line is back with its handle");
    assert_eq!(d.constraints.first().map(|c| c.id), Some(id), "the constraint keeps its local id");
    assert_no_dangling(d);
}

/// [`drawing`] with a two-by-two table (texts `a`, `b`, `c`, `d`, row by row) in model space.
fn table_pair() -> (Session, Session, kition_cad_doc::Handle) {
    let mut d = drawing();
    let cell = |text: &str| kition_cad_doc::TableCell { id: 0, text: text.into(), merged: None };
    let mut t = kition_cad_doc::Table {
        cell_seed: 0,
        insert: kition_cad_doc::geom::Vec3::new(300.0, 0.0, 0.0),
        col_widths: vec![20.0, 20.0],
        row_heights: vec![5.0, 5.0],
        cells: vec![vec![cell("a"), cell("b")], vec![cell("c"), cell("d")]],
        style: "Standard".into(),
        text_height: 2.5,
        title: false,
        header: false,
    };
    t.identify_cells().unwrap_or_else(|e| panic!("{e}"));
    let h = d.add(&Space::Model, on("0"), EntityKind::Table(t)).unwrap_or_else(|e| panic!("{e}"));
    let a = first(&d);
    let b = peer(&a, 2);
    (a, b, h)
}

#[test]
fn table_cell_texts_are_cell_objects_and_the_grid_holds_no_text() {
    let (a, _b, h) = table_pair();
    let replica = a.replica();
    let cells = replica.objects_of_type("TableCell");
    assert_eq!(cells.len(), 4, "one object per cell");
    let mut texts: Vec<String> = cells.iter().map(|c| replica.text(c, "text").unwrap_or_default()).collect();
    texts.sort();
    assert_eq!(texts, ["a", "b", "c", "d"]);
    let object = a.model().ids().entity(h).cloned().unwrap_or_else(|| panic!("table"));
    let kind = replica.get(&object, "kind").unwrap_or_default();
    assert!(!kind.to_string().contains("\"text\""), "the grid register holds no cell text: {kind}");
    let (read, _) = kdm_model::read_document(replica).unwrap_or_else(|error| panic!("read: {error}"));
    assert_eq!(canonical(&read), canonical(&a.model().drawing), "the texts read back from the cells");
}

#[test]
fn each_person_keeps_their_own_current_layer() {
    let (mut a, mut b) = pair();
    edit(&mut a, |d| d.header.set_str("CLAYER", "Walls"));
    sync(&mut a, &mut b);
    edit(&mut b, |d| {
        d.header.set_str("CLAYER", "Doors");
        let _ = d.add(&Space::Model, on("Doors"), line((1.0, 1.0), (2.0, 2.0)));
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_eq!(a.model().drawing.header.str("CLAYER", "?"), "Walls");
    assert_eq!(b.model().drawing.header.str("CLAYER", "?"), "Doors");
}

/// The current dimension style is each person's own: two people making different styles current
/// one after the other both keep theirs, the styles themselves are shared, and the replica holds
/// no DIMSTYLE.
#[test]
fn each_person_keeps_their_own_current_dimension_style() {
    let (mut a, mut b) = pair();
    edit(&mut a, |d| {
        for name in ["Arch", "Mech"] {
            d.dim_styles.push(kition_cad_doc::DimStyle { name: name.into(), ..kition_cad_doc::DimStyle::default() });
        }
        d.ensure_record_ids();
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    edit(&mut a, |d| d.header.set_str("DIMSTYLE", "Arch"));
    sync(&mut a, &mut b);
    edit(&mut b, |d| d.header.set_str("DIMSTYLE", "Mech"));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_eq!(a.model().drawing.header.str("DIMSTYLE", "?"), "Arch");
    assert_eq!(b.model().drawing.header.str("DIMSTYLE", "?"), "Mech");
    for s in [&a, &b] {
        assert!(s.model().drawing.dim_style("Arch").is_some() && s.model().drawing.dim_style("Mech").is_some(), "the styles are shared");
        let header = s.replica().map_entries(&kition_kit::kdm::ObjectId::from_string("drawing"), "header").unwrap_or_default();
        assert!(!header.iter().any(|(key, _)| key.eq_ignore_ascii_case("DIMSTYLE")), "the replica holds no current dimension style");
    }
}

/// The dimension settings (the DIM* variables but DIMSCALE) and the command defaults are each
/// person's own: two people setting different ones one after the other both keep theirs, and the
/// replica holds none of them; DIMSCALE is the drawing's and shared.
#[test]
fn each_person_keeps_their_own_dimension_settings_and_defaults() {
    let (mut a, mut b) = pair();
    edit(&mut a, |d| {
        d.header.set_f64("DIMASZ", 0.5);
        d.header.set_i64("DIMDEC", 2);
        d.header.set_f64("FILLETRAD", 1.0);
    });
    sync(&mut a, &mut b);
    edit(&mut b, |d| {
        d.header.set_f64("DIMASZ", 1.5);
        d.header.set_i64("DIMDEC", 3);
        d.header.set_f64("FILLETRAD", 2.0);
        d.header.set_f64("DIMSCALE", 4.0);
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let header = |s: &Session| s.model().drawing.header.clone();
    assert_eq!((header(&a).f64("DIMASZ", 0.0), header(&a).i64("DIMDEC", 0), header(&a).f64("FILLETRAD", 0.0)), (0.5, 2, 1.0));
    assert_eq!((header(&b).f64("DIMASZ", 0.0), header(&b).i64("DIMDEC", 0), header(&b).f64("FILLETRAD", 0.0)), (1.5, 3, 2.0));
    assert_eq!(header(&a).f64("DIMSCALE", 0.0), 4.0, "DIMSCALE is shared");
    for s in [&a, &b] {
        let held = s.replica().map_entries(&kition_kit::kdm::ObjectId::from_string("drawing"), "header").unwrap_or_default();
        let own: Vec<_> = held.iter().map(|(key, _)| key).filter(|key| kdm_model::is_current_var(key)).collect();
        assert!(own.is_empty(), "the replica holds nobody's own settings: {own:?}");
    }
}

/// One person's layer snapshots (LAYERP's previous layer state, LAYUNISO's state before
/// isolating), their own dimension scale and the current UCS they work in are each person's own
/// (kdm_model::CURRENT_VARS): two people setting different ones one after the other both keep
/// theirs, and the replica holds none of them; the named UCS records stay shared.
#[test]
fn each_person_keeps_their_own_layer_snapshots_dimension_scale_and_ucs() {
    use kition_cad_doc::HVal;
    use kition_cad_doc::geom::Vec3;
    let (mut a, mut b) = pair();
    edit(&mut a, |d| {
        d.header.set_str("KITION_CAD_LAYERP", "[]");
        d.header.set_str("KITION_CAD_LAYISO", "[]");
        d.header.set_f64(kition_cad_doc::OWN_DIMSCALE, 2.0);
        d.header.set("UCSORG", HVal::Point(Vec3::new(1.0, 2.0, 0.0)));
        d.header.set("PUCSXDIR", HVal::Point(Vec3::new(0.0, 1.0, 0.0)));
        d.header.set_str("UCSNAME", "Mine");
    });
    sync(&mut a, &mut b);
    edit(&mut b, |d| {
        d.header.set_f64(kition_cad_doc::OWN_DIMSCALE, 4.0);
        d.header.set("UCSORG", HVal::Point(Vec3::new(5.0, 5.0, 0.0)));
        d.ucss.push(kition_cad_doc::Ucs {
            name: "Shared".into(),
            origin: Vec3::new(3.0, 0.0, 0.0),
            x_axis: Vec3::new(1.0, 0.0, 0.0),
            y_axis: Vec3::new(0.0, 1.0, 0.0),
        });
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let (ha, hb) = (a.model().drawing.header.clone(), b.model().drawing.header.clone());
    assert_eq!(ha.str("KITION_CAD_LAYISO", "?"), "[]", "a's isolation snapshot is a's");
    assert_eq!(hb.get("KITION_CAD_LAYISO"), None, "b has no isolation snapshot of a's");
    assert_eq!(hb.get("KITION_CAD_LAYERP"), None, "b has no previous layer state of a's");
    assert_eq!((ha.f64(kition_cad_doc::OWN_DIMSCALE, 0.0), hb.f64(kition_cad_doc::OWN_DIMSCALE, 0.0)), (2.0, 4.0));
    assert_eq!(ha.get("UCSORG"), Some(&HVal::Point(Vec3::new(1.0, 2.0, 0.0))), "a works in a's UCS");
    assert_eq!(hb.get("UCSORG"), Some(&HVal::Point(Vec3::new(5.0, 5.0, 0.0))), "b works in b's UCS");
    assert_eq!((hb.get("UCSNAME"), hb.get("PUCSXDIR")), (None, None), "b has none of a's UCS settings");
    for s in [&a, &b] {
        assert!(s.model().drawing.ucss.iter().any(|u| u.name == "Shared"), "the named UCS is the drawing's, shared");
        let held = s.replica().map_entries(&kition_kit::kdm::ObjectId::from_string("drawing"), "header").unwrap_or_default();
        let own: Vec<_> = held.iter().map(|(key, _)| key).filter(|key| kdm_model::is_current_var(key)).collect();
        assert!(own.is_empty(), "the replica holds nobody's own settings: {own:?}");
    }
}
