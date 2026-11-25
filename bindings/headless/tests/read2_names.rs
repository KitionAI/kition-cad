//! C8-cad-read-2: case folding and record type are both parts of a named address.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "read2/common.rs"]
mod common;
use common::*;
use serde_json::json;

#[tokio::test(flavor = "multi_thread")]
async fn named_creations_hold_the_case_folded_record_type() {
    for command in ["block", "style", "dimensionStyle.open"] {
        check(
            |f| {
                (
                    command,
                    match command {
                        "block" => json!({"name":"free","base":[0,0],"handles":[f.line],"keep":"retain"}),
                        "style" => json!({"name":"free","height":2,"current":false}),
                        _ => json!({"name":"free","textHeight":2,"current":false}),
                    },
                )
            },
            |f| {
                vec![(
                    command,
                    match command {
                        "block" => json!({"name":"FREE","base":[100,100],"handles":[f.circle],"keep":"retain"}),
                        "style" => json!({"name":"FREE","height":3,"current":false}),
                        _ => json!({"name":"FREE","textHeight":3,"current":false}),
                    },
                )]
            },
            |f| {
                let mut edits = vec![if command == "style" {
                    ("layer.add", json!({"name":"free","color":3}))
                } else {
                    ("style", json!({"name":"free","current":false}))
                }];
                edits.push((
                    command,
                    match command {
                        "block" => json!({"name":"Other","base":[100,100],"handles":[f.circle],"keep":"retain"}),
                        _ => json!({"name":"Other","current":false}),
                    },
                ));
                edits
            },
        )
        .await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn block_name_replay_preserves_significant_whitespace() {
    check(
        |f| ("block", json!({"name":" free ","base":[0,0],"handles":[f.line],"keep":"retain"})),
        |f| vec![("block", json!({"name":" free ","base":[100,100],"handles":[f.circle],"keep":"retain"}))],
        |f| vec![("block", json!({"name":"free","base":[100,100],"handles":[f.circle],"keep":"retain"}))],
    )
    .await;
}
