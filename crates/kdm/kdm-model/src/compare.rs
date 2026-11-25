//! [`normalize`] and [`canonical`]: id-blind forms of a [`Drawing`], for comparing views.
//! Constraint ids are minted by every read (never replicated), the next handle is each
//! replica's own, and so are the current properties ([`crate::CURRENT_VARS`]), so two views of
//! the same content — a round trip, or two replicas that converged — rarely share them.

use kition_cad_doc::Drawing;
use serde_json::{Map, Value, json};

/// An id-blind copy of `d`: constraints renumbered from 1 in order, the next handle zeroed, and
/// each person's own current properties left out. Two drawings with the same content normalize
/// to the same value.
pub fn normalize(d: &Drawing) -> Drawing {
    let mut out = d.clone();
    for (i, c) in out.constraints.iter_mut().enumerate() {
        c.id = u32::try_from(i + 1).unwrap_or(u32::MAX);
    }
    out.handseed = 0;
    out.header.vars.retain(|key, _| !crate::is_current_var(key));
    out.record_ids.clear();
    out.record_ids_used.clear();
    out
}

/// [`normalize`]d `d` as JSON, numbers written the way a replica reads them back (a whole double
/// as an integer), for comparing two views with `==` and reading the difference.
pub fn canonical(d: &Drawing) -> Value {
    let d = normalize(d);
    // Every field, by name: a field added to `Drawing` fails to compile here until it is
    // compared (and, in the writer and reader, replicated).
    let Drawing {
        header,
        record_ids: _,
        record_ids_used: _,
        layers,
        linetypes,
        text_styles,
        dim_styles,
        mleader_styles,
        table_styles,
        blocks,
        model,
        layouts,
        views,
        ucss,
        layer_states,
        groups,
        plot_style_tables,
        constraints,
        parametric,
        handseed,
    } = &d;
    let store = |s: &kition_cad_doc::EntityStore| Value::Array(s.iter().map(|e| serde_json::to_value(&**e).unwrap_or(Value::Null)).collect());
    let blocks: Map<String, Value> = blocks
        .iter()
        .map(|(name, b)| (name.clone(), json!({ "block": serde_json::to_value(&**b).unwrap_or(Value::Null), "object.query": store(&b.entities) })))
        .collect();
    let layouts: Vec<Value> =
        layouts.iter().map(|l| json!({ "layout": serde_json::to_value(l).unwrap_or(Value::Null), "object.query": store(&l.entities) })).collect();
    let v = json!({
        "header": header,
        "layers": layers,
        "linetypes": linetypes,
        "textStyles": text_styles,
        "dimStyles": dim_styles,
        "mleaderStyles": mleader_styles,
        "tableStyles": table_styles,
        "blocks": blocks,
        "model": store(model),
        "layouts": layouts,
        "views": views,
        "ucss": ucss,
        "layerStates": layer_states,
        "groups": groups,
        "plotStyleTables": plot_style_tables,
        "constraints": constraints,
        "parametric": parametric,
        "handseed": handseed,
    });
    numbers(v)
}

/// Whole doubles within ±2^53 as integers (the KDM's canonical JSON), recursively.
pub(crate) fn numbers(v: Value) -> Value {
    const SAFE: f64 = 9_007_199_254_740_992.0;
    match v {
        Value::Number(n) if !n.is_i64() && !n.is_u64() => match n.as_f64() {
            Some(f) if f.fract() == 0.0 && f.abs() <= SAFE => Value::from(f as i64),
            _ => Value::Number(n),
        },
        Value::Array(items) => Value::Array(items.into_iter().map(numbers).collect()),
        Value::Object(map) => Value::Object(map.into_iter().map(|(k, v)| (k, numbers(v))).collect()),
        other => other,
    }
}
