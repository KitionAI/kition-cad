//! Independent same-server and outside writers, with a saved-merge oracle.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "../read2/common.rs"]
mod earlier;
pub use earlier::FILE;

pub fn content(d: &kition_cad_doc::Drawing) -> Value {
    earlier::content(d)
}

pub struct Fixture {
    core: earlier::Fixture,
    pub rect: Value,
    pub viewport: Value,
    pub hatch: Value,
    pub attached_dim: Value,
}
impl std::ops::Deref for Fixture {
    type Target = earlier::Fixture;
    fn deref(&self) -> &Self::Target {
        &self.core
    }
}
impl Fixture {
    pub fn new() -> Self {
        let core = earlier::Fixture::new();
        core.colleague(&[("rectangle.add", json!({"p1":[500,0],"p2":[510,10]}))]);
        core.colleague(&[
            ("viewport.add", json!({"layout":"Layout1","p1":[0,0],"p2":[20,20]})),
            ("hatch.add", json!({"handles":[core.circle],"pattern":"SOLID"})),
            ("arrange.group", json!({"name":"Group","handles":[core.line]})),
            ("dimension.addLinear", json!({"object":core.line,"at":[50,-10]})),
        ]);
        core.rewrite(|d| {
            let mut member = d.block("Marker").unwrap().entities.iter().next().unwrap().as_ref().clone();
            member.handle = d.new_handle();
            member.kind = kition_cad_doc::EntityKind::Line(kition_cad_doc::Line {
                a: serde_json::from_value(json!({"x":200.0,"y":200.0,"z":0.0})).unwrap(),
                b: serde_json::from_value(json!({"x":210.0,"y":200.0,"z":0.0})).unwrap(),
            });
            std::sync::Arc::make_mut(d.blocks.get_mut("Marker").unwrap()).entities.push(member);
        });
        let d = kition_cad_io::read(&core.disk(), FILE).unwrap();
        let rect = json!(d.model.iter().find(|e| matches!(e.kind, kition_cad_doc::EntityKind::LwPolyline(_))).unwrap().handle.hex());
        let viewport = json!(
            d.layouts
                .iter()
                .flat_map(|l| l.entities.iter())
                .find(|e| matches!(e.kind, kition_cad_doc::EntityKind::Viewport(_)))
                .unwrap()
                .handle
                .hex()
        );
        let hatch = json!(d.model.iter().find(|e| matches!(e.kind, kition_cad_doc::EntityKind::Hatch(_))).unwrap().handle.hex());
        let attached_dim = json!(
            d.model.iter().find(|e| matches!(&e.kind, kition_cad_doc::EntityKind::Dimension(dm) if !dm.assoc.is_empty())).unwrap().handle.hex()
        );
        Self { core, rect, viewport, hatch, attached_dim }
    }
}
use kition_cad_kit::CadDocument;
use kition_kit::KitDocument;
use kition_kits::tools::{self, CallOptions};
use serde_json::{Value, json};

pub async fn check(
    write: impl Fn(&Fixture) -> (&'static str, Value),
    necessary: impl Fn(&Fixture) -> Vec<(&'static str, Value)>,
    unrelated: impl Fn(&Fixture) -> Vec<(&'static str, Value)>,
) {
    check_with_setup(write, necessary, unrelated, |_| vec![]).await;
}

pub async fn check_with_setup(
    write: impl Fn(&Fixture) -> (&'static str, Value),
    necessary: impl Fn(&Fixture) -> Vec<(&'static str, Value)>,
    unrelated: impl Fn(&Fixture) -> Vec<(&'static str, Value)>,
    setup: impl Fn(&Fixture) -> Vec<(&'static str, Value)>,
) {
    check_program_with_setup(|f| vec![write(f)], necessary, unrelated, setup).await;
}

pub async fn check_program(
    program: impl Fn(&Fixture) -> Vec<(&'static str, Value)>,
    necessary: impl Fn(&Fixture) -> Vec<(&'static str, Value)>,
    unrelated: impl Fn(&Fixture) -> Vec<(&'static str, Value)>,
) {
    check_program_with_setup(program, necessary, unrelated, |_| vec![]).await;
}

async fn check_program_with_setup(
    program: impl Fn(&Fixture) -> Vec<(&'static str, Value)>,
    necessary: impl Fn(&Fixture) -> Vec<(&'static str, Value)>,
    unrelated: impl Fn(&Fixture) -> Vec<(&'static str, Value)>,
    setup: impl Fn(&Fixture) -> Vec<(&'static str, Value)>,
) {
    for outside in [false, true] {
        for refuse in [false, true] {
            let f = Fixture::new();
            let initial = setup(&f);
            if !initial.is_empty() {
                f.colleague(&initial);
            }
            let basis = f.call("inspect", json!({})).await.unwrap()["revision"].clone();
            let steps = if refuse { necessary(&f) } else { unrelated(&f) };
            let observed = f.disk();
            if outside {
                f.colleague(&steps);
            } else {
                let steps: Vec<_> = steps.iter().map(|(command, params)| json!({"command":command,"params":params})).collect();
                tools::call_as(&f.kits, "cad_run", &json!({"path":FILE,"steps":steps}), CallOptions::default(), "colleague").await.unwrap().unwrap();
            }
            let before = f.disk();
            assert!(observed != before, "the independent writer must really save a changed file");
            let writes = program(&f);
            let command = writes.iter().map(|(command, _)| *command).collect::<Vec<_>>().join(" -> ");
            let expected = if refuse {
                None
            } else {
                // The current layer, colour, styles, dimension settings… (kdm_model::is_current_var)
                // are each person's own: the agent keeps those of the file it observed, whatever
                // an outside writer saved as theirs, and its save writes its own.
                let observed = kition_cad_io::read(&observed, FILE).unwrap();
                let mut file = kition_cad_io::read(&before, FILE).unwrap();
                let oracle_bytes = if outside && kdm_model::current_vars(&observed) != kdm_model::current_vars(&file) {
                    kdm_model::carry_current(&observed, &mut file);
                    kition_cad_io::write(&file, FILE).unwrap()
                } else {
                    before.clone()
                };
                let mut oracle = CadDocument::open(FILE, &oracle_bytes).unwrap();
                for (command, params) in &writes {
                    oracle.execute(command, params.clone()).unwrap_or_else(|e| panic!("oracle {command}: {e}"));
                }
                Some(kition_cad_io::read(&oracle.save(Some(FILE)).unwrap(), FILE).unwrap())
            };
            let writes: Vec<_> = writes.into_iter().map(|(command, params)| json!({"command":command,"params":params})).collect();
            let answer = f.call("run", json!({"basis":basis,"steps":writes})).await;
            if refuse {
                let e = answer.expect_err(&format!("{command}, outside={outside}: a necessary fact changed"));
                assert_eq!(e["error"]["code"], "conflict", "{command}, outside={outside}: {e}");
                assert_eq!(before, f.disk());
            } else {
                answer.unwrap_or_else(|e| panic!("{command}, outside={outside}, unrelated={steps:?} must apply: {e}"));
                let actual = kition_cad_io::read(&f.disk(), FILE).unwrap();
                assert_eq!(earlier::content(&actual), earlier::content(expected.as_ref().unwrap()), "{command}, outside={outside}: saved merge");
                assert_eq!(actual.table_styles, expected.as_ref().unwrap().table_styles);
                assert_eq!(actual.mleader_styles, expected.as_ref().unwrap().mleader_styles);
            }
            f.kits.shutdown().await;
        }
    }
}
