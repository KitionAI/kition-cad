//! A dimension read from a file keeps the anonymous `*D` block it came with: saving writes that
//! block back and names it, instead of adding a generated block beside it on every save.

use kition_cad_doc::{Common, DimKind, Dimension, Drawing, EntityKind, Space};
use kition_cad_geom::Vec3;

fn dimension() -> Dimension {
    Dimension {
        kind: DimKind::Linear { rotation: 0.0 },
        defpt: Vec3::new(15.0, 5.0, 0.0),
        text_mid: Vec3::ZERO,
        p13: Vec3::ZERO,
        p14: Vec3::new(30.0, 0.0, 0.0),
        p15: Vec3::ZERO,
        p16: Vec3::ZERO,
        text: String::new(),
        style: "Standard".into(),
        measurement: 0.0,
        text_rotation: 0.0,
        user_text_pos: false,
        block: None,
        overrides: Default::default(),
        assoc: Vec::new(),
    }
}

fn anonymous_blocks(d: &Drawing) -> Vec<String> {
    d.blocks.values().filter(|b| b.anonymous).map(|b| b.name.clone()).collect()
}

fn block_of(d: &Drawing) -> Option<String> {
    d.model.iter().find_map(|e| if let EntityKind::Dimension(dm) = &e.kind { dm.block.clone() } else { None })
}

#[test]
fn a_read_dimension_keeps_its_block_through_saves() {
    let mut d = Drawing::new_imperial();
    d.add(&Space::Model, Common::default(), EntityKind::Dimension(dimension())).unwrap();
    let once = kition_cad_io::read_dxf(kition_cad_io::write_dxf(&d).as_bytes()).unwrap();
    assert_eq!(anonymous_blocks(&once), vec!["*D1".to_string()]);
    assert_eq!(block_of(&once).as_deref(), Some("*D1"));
    let twice = kition_cad_io::read_dxf(kition_cad_io::write_dxf(&once).as_bytes()).unwrap();
    assert_eq!(anonymous_blocks(&twice), vec!["*D1".to_string()], "no second block per dimension");
    assert_eq!(block_of(&twice).as_deref(), Some("*D1"));
    let a = once.blocks.get("*D1").unwrap();
    let b = twice.blocks.get("*D1").unwrap();
    assert_eq!(a.entities.len(), b.entities.len());
}

#[test]
fn an_edited_dimension_gets_a_fresh_block() {
    let mut d = Drawing::new_imperial();
    d.add(&Space::Model, Common::default(), EntityKind::Dimension(dimension())).unwrap();
    let mut once = kition_cad_io::read_dxf(kition_cad_io::write_dxf(&d).as_bytes()).unwrap();
    // An edit clears the block (the commands do), so the next save draws the dimension anew.
    let h = once.model.iter().map(|e| e.handle).next().unwrap();
    once.modify_entity(h, |e| {
        if let EntityKind::Dimension(dm) = &mut e.kind {
            dm.p14 = Vec3::new(40.0, 0.0, 0.0);
            dm.block = None;
        }
    })
    .unwrap();
    let text = kition_cad_io::write_dxf(&once);
    let twice = kition_cad_io::read_dxf(text.as_bytes()).unwrap();
    let name = block_of(&twice).unwrap();
    assert_ne!(name, "*D1", "the stale block is not reused");
    assert!(twice.blocks.contains_key(&name));
}
