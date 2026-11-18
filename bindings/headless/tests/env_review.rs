//! Reproducible environment inventory using the kit's pinned SDK, without model calls.
#![allow(clippy::unwrap_used, clippy::panic)]
use kition_cad_kit::CadDocument;
use kition_env::{EnvKit, GenerateOptions, generate, stats};
use kition_kit::KitDocument;
use kition_kits::KitDefinition;
use serde_json::json;
use std::path::PathBuf;

#[tokio::test(flavor = "multi_thread")]
async fn pinned_environment_inventory() {
    let kit = KitDefinition::from_manifest_text(include_str!("../../../kit.json")).unwrap();
    let kit = EnvKit::new(kit, Some(PathBuf::from(env!("CARGO_BIN_EXE_kition-cad"))));
    let count = std::env::var("C8_ENV_COUNT").ok().and_then(|n| n.parse().ok()).unwrap_or(10);
    let seed = std::env::var("C8_ENV_SEED").ok().and_then(|n| n.parse().ok()).unwrap_or(42);
    let result =
        generate(&kit, &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../conformance"), &GenerateOptions { seed, count, ..Default::default() })
            .await
            .unwrap();
    println!(
        "PINNED ENV {} given_up={} skipped={} excluded={:?}",
        stats(&result.tasks),
        result.given_up.len(),
        result.skipped.len(),
        result.excluded
    );
    for task in &result.tasks {
        println!("ENV TASK {}", task.to_line());
    }
    for task in &result.given_up {
        println!("ENV GIVEN UP {task}");
    }
    assert!(!result.tasks.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn json_sampling_and_required_json_creation_replay() {
    let definition = KitDefinition::from_manifest_text(include_str!("../../../kit.json")).unwrap();
    let kit = EnvKit::new(definition, Some(PathBuf::from(env!("CARGO_BIN_EXE_kition-cad"))));
    let dir = tempfile::tempdir().unwrap();
    let mut d = CadDocument::create("Words.dxf", 420, 297).unwrap();
    for (y, text) in [(0, "Door"), (20, "Window")] {
        d.execute("text", json!({"at":[0,y],"text":text})).unwrap();
    }
    std::fs::write(dir.path().join("Words.dxf"), d.save(Some("Words.dxf")).unwrap()).unwrap();
    for kind in ["set-field", "replace-text", "create"] {
        let result = generate(
            &kit,
            dir.path(),
            &GenerateOptions { seed: 42, count: 1, inverse_share: 0.0, kinds: Some(vec![kind.into()]), ..Default::default() },
        )
        .await
        .unwrap();
        assert_eq!(result.tasks.len(), 1, "{kind}: {:?}", result.given_up);
        assert!(result.given_up.is_empty(), "{kind}: {:?}", result.given_up);
        let task = &result.tasks[0];
        let replay = kition_env::trace(&kit, dir.path(), task, &mut kition_env::Oracle::new(task, Default::default())).await.unwrap();
        assert!(replay.verified(), "{kind}: {}", replay.to_line());
        assert_eq!(replay.reward.preserved, 1.0, "{kind}: unrelated content must survive replay");
    }
}
