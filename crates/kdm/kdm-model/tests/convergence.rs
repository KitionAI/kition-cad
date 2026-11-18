//! Seeded random multi-session convergence: ~200 random model-level edits on two or three
//! sessions with random exchanges and undos; after a final exchange every replica has the same
//! state hash and the same (id-blind) view, the view is what a fresh read of its replica gives,
//! and no view or replica holds a dangling reference.

mod common;

use std::sync::Arc;

use common::{Session, assert_no_dangling, assert_replica_clean, canonical, circle, differences, drawing, edit, first, line, on, peer, sync_all};
use kition_cad_doc::geom::{Mat3, Vec2, Vec3};
use kition_cad_doc::{Block, Constraint, ConstraintKind, Drawing, EntityKind, GeomRef, Group, Handle, Insert, Layer, Space};

/// xorshift64*: deterministic, no dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        if n == 0 { 0 } else { (self.next() % n as u64) as usize }
    }
    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }
    fn coord(&mut self) -> f64 {
        self.below(400) as f64
    }
}

fn model_handles(d: &Drawing) -> Vec<Handle> {
    d.model.handles()
}

/// What the engine's own commands do after deleting entities: constraints, group members and
/// dimension associations naming them go too.
fn tidy(d: &mut Drawing) {
    let all: std::collections::HashSet<Handle> = std::iter::once(&d.model)
        .chain(d.layouts.iter().map(|l| &l.entities))
        .chain(d.blocks.values().map(|b| &b.entities))
        .flat_map(|s| s.iter().map(|e| e.handle))
        .collect();
    d.constraints.retain(|c| c.refs.iter().all(|r| all.contains(&r.handle)));
    for g in &mut d.groups {
        g.members.retain(|h| all.contains(h));
    }
    d.groups.retain(|g| !g.members.is_empty());
    let dims: Vec<Handle> = d.model.iter().filter(|e| matches!(e.kind, EntityKind::Dimension(_))).map(|e| e.handle).collect();
    for h in dims {
        d.model.modify(h, |e| {
            if let EntityKind::Dimension(dim) = &mut e.kind {
                dim.assoc.retain(|a| {
                    all.contains(&a.handle) && !matches!(a.snap, kition_cad_doc::AssocSnap::Intersection { other } if !all.contains(&other))
                });
            }
        });
    }
}

fn some_layer(d: &Drawing, rng: &mut Rng) -> String {
    d.layers.get(rng.below(d.layers.len())).map_or_else(|| "0".to_owned(), |l| l.name.clone())
}

fn random_edit(d: &mut Drawing, rng: &mut Rng) {
    let hs = model_handles(d);
    let pick = |rng: &mut Rng| hs.get(rng.below(hs.len())).copied();
    match rng.below(18) {
        0 | 1 => {
            let layer = some_layer(d, rng);
            let (a, b) = ((rng.coord(), rng.coord()), (rng.coord() + 1.0, rng.coord()));
            let _ = d.add(&Space::Model, on(&layer), line(a, b));
        }
        2 => {
            let layer = some_layer(d, rng);
            let c = (rng.coord(), rng.coord());
            let _ = d.add(&Space::Model, on(&layer), circle(c, 1.0 + rng.below(30) as f64));
        }
        3 => {
            if let Some(h) = pick(rng) {
                d.model.remove(h);
                tidy(d);
            }
        }
        4 | 5 => {
            if let Some(h) = pick(rng) {
                let m = Mat3::translate(Vec2::new(rng.below(20) as f64 - 10.0, rng.below(20) as f64 - 10.0));
                d.model.modify(h, |e| e.kind.transform(&m));
            }
        }
        6 => {
            if let Some(h) = pick(rng) {
                let color = kition_cad_doc::color::Color::Index(1 + rng.below(200) as u8);
                d.model.modify(h, |e| e.common.color = color);
            }
        }
        7 => {
            if let Some(h) = pick(rng) {
                if rng.chance(50) {
                    d.model.bring_to_front(h);
                } else {
                    d.model.send_to_back(h);
                }
            }
        }
        8 => {
            let name = format!("L{}", rng.below(8));
            if d.layer(&name).is_none() {
                d.layers.push(Layer::new(&name));
            }
        }
        9 => {
            // Delete an unused layer (the engine refuses a used one, the current one and 0).
            let used = |d: &Drawing, n: &str| {
                std::iter::once(&d.model)
                    .chain(d.layouts.iter().map(|l| &l.entities))
                    .chain(d.blocks.values().map(|b| &b.entities))
                    .any(|s| s.iter().any(|e| e.common.layer.eq_ignore_ascii_case(n)))
            };
            let current = d.header.str("CLAYER", "0");
            let candidates: Vec<String> = d
                .layers
                .iter()
                .map(|l| l.name.clone())
                .filter(|n| n != "0" && n != "Defpoints" && !n.eq_ignore_ascii_case(&current) && !used(d, n))
                .collect();
            if let Some(name) = candidates.get(rng.below(candidates.len())).cloned() {
                d.layers.retain(|l| l.name != name);
            }
        }
        10 => {
            // Rename a layer, re-pointing everything on it (RENAME).
            let i = rng.below(d.layers.len());
            let Some(from) = d.layers.get(i).map(|l| l.name.clone()).filter(|n| n != "0") else { return };
            let to = format!("R{}", rng.below(100));
            if d.layer(&to).is_some() {
                return;
            }
            if let Some(l) = d.layers.get_mut(i) {
                l.name = to.clone();
            }
            for h in model_handles(d) {
                d.model.modify(h, |e| {
                    if e.common.layer == from {
                        e.common.layer = to.clone();
                    }
                });
            }
            if d.header.str("CLAYER", "0") == from {
                d.header.set_str("CLAYER", &to);
            }
        }
        11 => {
            let lines: Vec<Handle> = d.model.iter().filter(|e| matches!(e.kind, EntityKind::Line(_))).map(|e| e.handle).collect();
            if let Some(h) = lines.get(rng.below(lines.len())).copied() {
                let id = d.constraints.iter().map(|c| c.id).max().unwrap_or(0) + 1;
                d.constraints.push(Constraint {
                    id,
                    kind: ConstraintKind::Fix,
                    refs: vec![GeomRef::whole(h)],
                    name: String::new(),
                    expr: String::new(),
                });
            }
        }
        12 => {
            if !d.constraints.is_empty() {
                let i = rng.below(d.constraints.len());
                d.constraints.remove(i);
            }
        }
        13 => {
            let (Some(a), Some(b)) = (pick(rng), pick(rng)) else { return };
            let name = if rng.chance(50) { format!("G{}", rng.below(5)) } else { format!("*A{}", d.groups.len() + 1) };
            if !name.starts_with('*') && d.groups.iter().any(|g| g.name == name) {
                return;
            }
            let members = if a == b { vec![a] } else { vec![a, b] };
            d.groups.push(Group { name, description: String::new(), selectable: true, members });
        }
        14 => d.header.set_f64("LTSCALE", 0.5 + rng.below(8) as f64),
        15 => {
            // Make a block of one entity (removed from model space) and insert it.
            let Some(h) = pick(rng) else { return };
            let name = format!("B{}", rng.below(4));
            if d.block(&name).is_some() {
                return;
            }
            let Some(e) = d.model.remove(h) else { return };
            tidy(d);
            let mut block = Block::new(&name);
            block.entities.push((*e).clone());
            d.blocks.insert(name.clone(), Arc::new(block));
            let insert = Insert {
                block: name,
                insert: Vec3::new(rng.coord(), rng.coord(), 0.0),
                scale: Vec3::new(1.0, 1.0, 1.0),
                rotation: 0.0,
                attribs: Vec::new(),
                cols: 1,
                rows: 1,
                col_spacing: 0.0,
                row_spacing: 0.0,
            };
            let _ = d.add(&Space::Model, on("0"), EntityKind::Insert(insert));
        }
        16 => {
            if d.layout("Layout1").is_some() {
                let c = (rng.coord(), rng.coord());
                let _ = d.add(&Space::Paper("Layout1".into()), on("0"), circle(c, 3.0));
            }
        }
        _ => {
            let i = rng.below(d.layers.len());
            let color = kition_cad_doc::color::Color::Index(1 + rng.below(200) as u8);
            if let Some(l) = d.layers.get_mut(i) {
                l.color = color;
            }
        }
    }
}

fn run(seed: u64, sessions: usize, steps: usize) {
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let a = first(&drawing());
    let mut all: Vec<Session> = vec![a];
    for k in 1..sessions {
        let next = peer(&all[0], 10 + k as u64);
        all.push(next);
    }
    for _ in 0..steps {
        let who = rng.below(all.len());
        let sub = rng.next();
        edit(&mut all[who], |d| random_edit(d, &mut Rng(sub | 1)));
        if rng.chance(25) {
            let (from, to) = (rng.below(all.len()), rng.below(all.len()));
            if from != to {
                let bytes = all[from].export_changes(&all[to].version()).unwrap_or_else(|error| panic!("export: {error}"));
                all[to].import_changes(&bytes).unwrap_or_else(|error| panic!("seed {seed}: import: {error}"));
            }
        }
        if rng.chance(3) {
            let who = rng.below(all.len());
            all[who].undo().unwrap_or_else(|error| panic!("seed {seed}: undo: {error}"));
        }
    }
    sync_all(&mut all);
    let hash = all[0].state_hash().unwrap_or_else(|error| panic!("hash: {error}"));
    let view = canonical(&all[0].model().drawing);
    for (k, session) in all.iter().enumerate() {
        assert_eq!(session.state_hash().unwrap_or_else(|error| panic!("hash: {error}")), hash, "seed {seed}: session {k} state hash");
        let mine = canonical(&session.model().drawing);
        assert!(mine == view, "seed {seed}: session {k} view:\n{:#?}", differences(&mine, &view));
        assert_no_dangling(&session.model().drawing);
        assert_replica_clean(session.replica());
        // The view is what a fresh read of the replica gives.
        let (fresh, _) = kdm_model::read_document(session.replica()).unwrap_or_else(|error| panic!("read: {error}"));
        let fresh = canonical(&fresh);
        assert!(fresh == view, "seed {seed}: session {k} view is not its replica:\n{:#?}", differences(&fresh, &view));
    }
}

#[test]
fn two_sessions_converge() {
    for seed in [1, 2, 3] {
        run(seed, 2, 200);
    }
}

#[test]
fn three_sessions_converge() {
    for seed in [11, 12] {
        run(seed, 3, 200);
    }
}
